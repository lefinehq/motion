//! Corner pins ([D89]): where each `corner-pin` effect's corners rest and
//! lie at one frame, resolved once in evaluation so the render, `check` and
//! motion blur read the same quad.
//!
//! A corner rests on the corner of the layer's effect box, or, when the pin
//! names `pivots`, where its pivot's anchor lands on the composition's
//! canvas, taken into the pinned layer's own coordinates. Its value then
//! moves it from there.
//!
//! [D89]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography

use std::fmt;

use glam::{DMat3, DVec3};
use kurbo::{Affine, Point, Rect, Vec2};
use nakkash_format::property::Prop;

use crate::space;
use crate::{CompGraph, ContentState, ItemState, Node, NodeState, SceneGraph};

/// The name of the corner pin's kind.
pub const CORNER_PIN: &str = "corner-pin";

/// One corner of a pin, in the order its rows and pivots are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Corner {
    /// The top-left corner of the box.
    TopLeft,
    /// The top-right corner.
    TopRight,
    /// The bottom-right corner.
    BottomRight,
    /// The bottom-left corner.
    BottomLeft,
}

impl Corner {
    /// Every corner, top left first and clockwise.
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomRight,
        Corner::BottomLeft,
    ];

    /// Its name, which is also its row's.
    pub fn name(self) -> &'static str {
        match self {
            Corner::TopLeft => "top-left",
            Corner::TopRight => "top-right",
            Corner::BottomRight => "bottom-right",
            Corner::BottomLeft => "bottom-left",
        }
    }
}

impl fmt::Display for Corner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Why a pin's corners make no quad it can lay its box onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Two opposite sides cross, each named by the corners it joins: the
    /// corners are not in order round the quad.
    Crossed([[Corner; 2]; 2]),
    /// The corner lies inside the triangle of the other three: the map
    /// would fold part of the box through the horizon.
    Bent(Corner),
    /// The corner lies on the line through its neighbours, or on one of
    /// them: the map has no inverse.
    Flat(Corner),
    /// The pivot of this corner cannot be seen from the pinned layer: the
    /// layer has no inverse there, or the pivot lies behind its horizon.
    Unplaced(Corner),
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Crossed([[a, b], [c, d]]) => write!(f, "crossed: {a}–{b} meets {c}–{d}"),
            Fault::Bent(corner) => write!(f, "bent: {corner} inside the others"),
            Fault::Flat(corner) => write!(f, "flat: {corner} in line with its neighbours"),
            Fault::Unplaced(corner) => write!(f, "unplaced: {corner} not on the layer's plane"),
        }
    }
}

/// One corner pin of a node at one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pin {
    /// Its place among the node's shown effects, its `fx`.
    pub fx: usize,
    /// The box its corners are pinned from, in the layer's own coordinates:
    /// its effect box ([`effect_box`]).
    pub from: Rect,
    /// Where the box's corners go, in the layer's own coordinates, top left
    /// first and clockwise.
    pub to: [Point; 4],
    /// Why `to` makes no quad, when it does not.
    pub fault: Option<Fault>,
}

impl Pin {
    /// The corners of `from`, top left first and clockwise.
    pub fn rest(&self) -> [Point; 4] {
        corners_of(self.from)
    }

    /// How far each corner moves from the corner of `from` it starts at.
    pub fn offsets(&self) -> [Vec2; 4] {
        let rest = self.rest();
        std::array::from_fn(|i| self.to[i] - rest[i])
    }
}

/// The corners of `rect`, top left first and clockwise.
fn corners_of(rect: Rect) -> [Point; 4] {
    [
        Point::new(rect.x0, rect.y0),
        Point::new(rect.x1, rect.y0),
        Point::new(rect.x1, rect.y1),
        Point::new(rect.x0, rect.y1),
    ]
}

/// A node's effect box in its own coordinates: what its content fills (a
/// rect, an image's or video's frame, a composition's canvas), or for
/// content without a box of its own, `around`, the canvas it sits in, with
/// its middle on the node's origin.
pub fn effect_box(graph: &SceneGraph, node: &NodeState, around: Rect) -> Rect {
    match &node.content {
        ContentState::Rect(shape) => Rect::new(0.0, 0.0, shape.width, shape.height),
        ContentState::Image(content) => content.frame,
        ContentState::Video { video, .. } => video.frame,
        ContentState::Comp(index, _) => {
            let inner = &graph.comps[*index];
            Rect::new(0.0, 0.0, inner.width, inner.height)
        }
        _ => around - around.center().to_vec2(),
    }
}

/// Where node `i` of a composition's nodes lands its anchor on the
/// composition's canvas: through its parents, and its plane when it is
/// 3D. `None` when it lands behind the camera.
fn landing(node: &NodeState) -> Option<Point> {
    let [x, y] = node.point(Prop::ANCHOR);
    let anchor = Point::new(x, y);
    match node.plane {
        Some(plane) => space::project(plane, anchor),
        None => Some(node.transform * anchor),
    }
}

/// From the composition's canvas back to `node`'s own coordinates: the
/// inverse of its transform, or of its plane when it is 3D. `None` where
/// the layer has no inverse.
fn back(node: &NodeState) -> Option<Back> {
    match node.plane {
        Some(plane) => {
            let inverse = plane.inverse();
            (plane.determinant().abs() > f64::MIN_POSITIVE && inverse.is_finite())
                .then_some(Back::Plane(inverse))
        }
        None => {
            let det = node.transform.determinant();
            (det.abs() > f64::MIN_POSITIVE && det.is_finite())
                .then(|| Back::Flat(node.transform.inverse()))
        }
    }
}

/// The map from a composition's canvas to a layer's own coordinates.
enum Back {
    /// An affine map, for a flat layer.
    Flat(Affine),
    /// A projective one, for a 3D layer.
    Plane(DMat3),
}

impl Back {
    /// Where `p` of the canvas is on the layer, or `None` when it is on
    /// or beyond the plane's horizon.
    fn map(&self, p: Point) -> Option<Point> {
        match self {
            Back::Flat(affine) => Some(*affine * p),
            Back::Plane(inverse) => {
                let v = *inverse * DVec3::new(p.x, p.y, 1.0);
                (v.z > 0.0).then(|| Point::new(v.x / v.z, v.y / v.z))
            }
        }
    }
}

/// The corner pins of node `i` of `nodes`, the composition `comp` of
/// `graph` at one frame, compiled as `compiled`: one for each of its shown
/// `corner-pin` effects, in order.
pub(crate) fn pins(
    graph: &SceneGraph,
    comp: &CompGraph,
    compiled: &Node,
    nodes: &[NodeState],
    i: usize,
) -> Vec<Pin> {
    let node = &nodes[i];
    let around = Rect::new(0.0, 0.0, comp.width, comp.height);
    // A collapsed composition runs its effects in the coordinates of the
    // canvas it draws into, on its canvas where it lands there.
    let collapsed = compiled.collapse && matches!(node.content, ContentState::Comp(..));
    let (from, back) = match collapsed {
        true => (
            node.transform
                .transform_rect_bbox(effect_box(graph, node, around)),
            Some(Back::Flat(Affine::IDENTITY)),
        ),
        false => (effect_box(graph, node, around), back(node)),
    };
    node.fx
        .iter()
        .enumerate()
        .filter(|(_, item)| item.kind.name == CORNER_PIN)
        .map(|(fx, item)| {
            let pivots = compiled.pivots.get(fx).copied().flatten();
            pin(fx, item, from, pivots, nodes, back.as_ref())
        })
        .collect()
}

/// One pin: its rest moved by its values, resting on `pivots`' landings
/// when it names them.
fn pin(
    fx: usize,
    item: &ItemState,
    from: Rect,
    pivots: Option<[usize; 4]>,
    nodes: &[NodeState],
    back: Option<&Back>,
) -> Pin {
    let offset = |k: usize| {
        let [x, y] = item.values[k].as_point().expect("a corner is a point");
        Vec2::new(x, y)
    };
    let mut rest = corners_of(from);
    let mut fault = None;
    if let Some(pivots) = pivots {
        for (k, &source) in pivots.iter().enumerate() {
            let seen = landing(&nodes[source]).and_then(|p| back?.map(p));
            match seen {
                Some(p) => rest[k] = p,
                None => {
                    fault.get_or_insert(Fault::Unplaced(Corner::ALL[k]));
                }
            }
        }
    }
    let to: [Point; 4] = std::array::from_fn(|k| rest[k] + offset(k));
    Pin {
        fx,
        from,
        to,
        fault: fault.or_else(|| quad_fault(to)),
    }
}

/// The least sine of the angle at a corner, between the sides that meet
/// there, for the corner to count as a corner and not a point on a line.
/// A homography onto a corner this flat stretches its neighbourhood by
/// about its inverse, so 1e-4 already spreads a px of the box over ten
/// thousand.
const LEAST_SINE: f64 = 1e-4;

/// Why the quad `q`, top left first and clockwise, cannot take a box
/// through a homography that keeps the box in front of its horizon:
/// `None` for a convex quad, turning either way (a mirrored pin is a
/// pin).
pub fn quad_fault(q: [Point; 4]) -> Option<Fault> {
    let side = |k: usize| q[(k + 1) % 4] - q[k];
    // The turn at each corner, from the side coming in to the side going out.
    let mut turns = [0.0; 4];
    for (k, turn) in turns.iter_mut().enumerate() {
        let (into, out) = (side((k + 3) % 4), side(k));
        let lengths = into.hypot() * out.hypot();
        let cross = into.cross(out);
        if !(lengths.is_finite() && cross.is_finite())
            || lengths == 0.0
            || cross.abs() <= LEAST_SINE * lengths
        {
            return Some(Fault::Flat(Corner::ALL[k]));
        }
        *turn = cross;
    }
    let positive = turns.iter().filter(|t| **t > 0.0).count();
    match positive {
        0 | 4 => None,
        1 | 3 => {
            // The one turning against the others lies inside them.
            let odd = turns
                .iter()
                .position(|t| (*t > 0.0) == (positive == 1))
                .expect("one turn differs");
            Some(Fault::Bent(Corner::ALL[odd]))
        }
        _ => {
            // Two opposite sides cross; name the pair that does.
            let crosses = |a: usize, b: usize| {
                let (p, r) = (q[a], side(a));
                let (s, t) = (q[b], side(b));
                let denom = r.cross(t);
                if denom == 0.0 {
                    return false;
                }
                let u = (s - p).cross(t) / denom;
                let v = (s - p).cross(r) / denom;
                (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)
            };
            let pair = |a: usize| {
                let ends = |k: usize| [Corner::ALL[k], Corner::ALL[(k + 1) % 4]];
                [ends(a), ends(a + 2)]
            };
            Some(Fault::Crossed(match crosses(0, 2) {
                true => pair(0),
                false => pair(1),
            }))
        }
    }
}

/// Every pin of a composition's nodes at one frame, in node order: the
/// pins of node `i` are the `i`th.
pub(crate) fn of_frame(graph: &SceneGraph, comp: &CompGraph, nodes: &[NodeState]) -> Vec<Vec<Pin>> {
    (0..nodes.len())
        .map(|i| match has_pin(&nodes[i]) {
            true => pins(graph, comp, &comp.nodes[i], nodes, i),
            false => Vec::new(),
        })
        .collect()
}

/// Whether `node` has a corner pin among its effects.
fn has_pin(node: &NodeState) -> bool {
    node.fx.iter().any(|item| item.kind.name == CORNER_PIN)
}

#[cfg(test)]
#[path = "../tests/unit/pin.rs"]
mod tests;
