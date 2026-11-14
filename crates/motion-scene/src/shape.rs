//! Shape items resolved into what a renderer draws: each paint with the
//! outlines it covers, back to front.
//!
//! Within a group the first item is on top. A paint draws every outline
//! listed before it in its group, those inside earlier groups included,
//! through those groups' transforms. It strokes and fills in its own group's
//! space and takes the opacity of every group around it.

use kurbo::{Affine, BezPath, Point, Rect, Shape, Vec2};
use motion_format::item::{Role, props};
use motion_format::property::GROUP;

use crate::evaluate::local_transform;
use crate::state::ItemState;

/// The share of a quarter circle's radius at which a cubic's handles sit so
/// that it follows the arc: `4/3 (√2 − 1)`.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// One paint and the outlines it covers.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeDraw {
    /// The fill or stroke, without items.
    pub paint: ItemState,
    /// Every outline it covers, as one path in the paint's group space, so a
    /// fill rule counts across them.
    pub outline: BezPath,
    /// From the paint's group space to the layer's own coordinates.
    pub transform: Affine,
    /// The product of the opacities of the groups around the paint.
    pub opacity: f64,
}

impl ShapeDraw {
    /// The box the draw covers in the layer's own coordinates: the outline,
    /// widened by half a stroke's width.
    pub fn bounds(&self) -> Rect {
        let outline = self.outline.bounding_box();
        let reach = match std::ptr::eq(self.paint.kind.registry, props::STROKE_WIDTH.registry()) {
            true => self.paint.number(props::STROKE_WIDTH) / 2.0,
            false => 0.0,
        };
        self.transform
            .transform_rect_bbox(outline.inflate(reach, reach))
    }
}

/// Draws the outline of one geometry item.
type Draw = fn(&ItemState) -> BezPath;

/// What each geometry kind draws, by name; every geometry kind has a row.
const OUTLINES: [(&str, Draw); 3] = [("rect", rect), ("ellipse", ellipse), ("path", path)];

/// The outline a geometry item draws, in its group's space; `None` for
/// any other item.
///
/// # Panics
/// If a geometry kind has no row in the outline table, which a test rules out.
pub fn outline(item: &ItemState) -> Option<BezPath> {
    if item.kind.role != Role::Geometry {
        return None;
    }
    let (_, draw) = OUTLINES
        .iter()
        .find(|(name, _)| *name == item.kind.name)
        .expect("every geometry kind has an outline");
    Some(draw(item))
}

/// Every draw of `items`, back to front.
pub(crate) fn draws(items: &[ItemState]) -> Vec<ShapeDraw> {
    let mut top_down = Vec::new();
    group_draws(items, Affine::IDENTITY, 1.0, &mut top_down);
    top_down.reverse();
    top_down
}

/// The draws of one group's `items`, top first, into `out`: `transform` takes
/// the group's space to the layer's and `opacity` is the groups' product.
fn group_draws(items: &[ItemState], transform: Affine, opacity: f64, out: &mut Vec<ShapeDraw>) {
    let mut above = BezPath::new();
    for item in items {
        match item.kind.role {
            Role::Geometry => above.extend(outline(item).expect("a geometry item")),
            Role::Group => {
                let (own, own_opacity) = group_transform(item);
                group_draws(&item.items, transform * own, opacity * own_opacity, out);
                above.extend(own * geometry(&item.items));
            }
            Role::Paint if !above.elements().is_empty() => out.push(ShapeDraw {
                paint: ItemState {
                    items: Vec::new(),
                    ..item.clone()
                },
                outline: above.clone(),
                transform,
                opacity,
            }),
            Role::Paint | Role::Effect => {}
        }
    }
}

/// Every outline in `items` and their groups, in the space they sit in.
fn geometry(items: &[ItemState]) -> BezPath {
    let mut all = BezPath::new();
    for item in items {
        match item.kind.role {
            Role::Geometry => all.extend(outline(item).expect("a geometry item")),
            Role::Group => all.extend(group_transform(item).0 * geometry(&item.items)),
            Role::Paint | Role::Effect => {}
        }
    }
    all
}

/// A group's transform into the space around it, and its opacity. Its rows
/// are a layer's transform rows, so the layer's transform reads them.
fn group_transform(group: &ItemState) -> (Affine, f64) {
    debug_assert!(std::ptr::eq(group.kind.registry, &GROUP));
    let opacity = group.values[motion_format::property::Prop::OPACITY.index()]
        .as_number()
        .expect("a number property");
    (local_transform(&group.values), opacity)
}

/// A rectangle about its centre, clockwise from the top of its right edge,
/// with corners rounded by `round`, capped at half the shorter side.
fn rect(item: &ItemState) -> BezPath {
    let [w, h] = item.point(props::RECT_SIZE);
    let [cx, cy] = item.point(props::RECT_POSITION);
    let r = item.number(props::RECT_ROUND).min(w / 2.0).min(h / 2.0);
    let (x0, y0, x1, y1) = (cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0);
    let mut path = BezPath::new();
    path.move_to((x1, y0 + r));
    // Each side, then the corner after it: the corner's point and the
    // directions from it back along the side and on along the next.
    let corners = [
        ((x1, y1), Vec2::new(0.0, -1.0), Vec2::new(-1.0, 0.0)),
        ((x0, y1), Vec2::new(1.0, 0.0), Vec2::new(0.0, -1.0)),
        ((x0, y0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)),
        ((x1, y0), Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0)),
    ];
    for (corner, back, on) in corners {
        let corner = Point::from(corner);
        let (enter, leave) = (corner + back * r, corner + on * r);
        path.line_to(enter);
        if r > 0.0 {
            let k = r * (1.0 - KAPPA);
            path.curve_to(corner + back * k, corner + on * k, leave);
        }
    }
    path.close_path();
    path
}

/// An ellipse about its centre: four quarter arcs clockwise from the top.
fn ellipse(item: &ItemState) -> BezPath {
    let [w, h] = item.point(props::ELLIPSE_SIZE);
    let [cx, cy] = item.point(props::ELLIPSE_POSITION);
    let (rx, ry) = (w / 2.0, h / 2.0);
    let c = Point::new(cx, cy);
    // The four extremes in order, each with the direction of travel through it.
    let at = [
        (Vec2::new(0.0, -ry), Vec2::new(rx, 0.0)),
        (Vec2::new(rx, 0.0), Vec2::new(0.0, ry)),
        (Vec2::new(0.0, ry), Vec2::new(-rx, 0.0)),
        (Vec2::new(-rx, 0.0), Vec2::new(0.0, -ry)),
    ];
    let mut path = BezPath::new();
    path.move_to(c + at[0].0);
    for i in 0..4 {
        let (from, from_dir) = at[i];
        let (to, to_dir) = at[(i + 1) % 4];
        path.curve_to(c + from + from_dir * KAPPA, c + to - to_dir * KAPPA, c + to);
    }
    path.close_path();
    path
}

/// A free path: each vertex to the next through its leaving handle and the
/// next one's arriving handle, and back to the first when closed.
fn path(item: &ItemState) -> BezPath {
    let shape = item.path(props::PATH_PATH);
    let vertices: Vec<_> = shape.vertices().collect();
    let mut out = BezPath::new();
    let Some(first) = vertices.first() else {
        return out;
    };
    let p = |v: [f64; 2]| Point::new(v[0], v[1]);
    let plus = |a: [f64; 2], b: [f64; 2]| Point::new(a[0] + b[0], a[1] + b[1]);
    out.move_to(p(first.point));
    let closing = shape.closed().then_some(first);
    for (from, to) in vertices.iter().zip(vertices.iter().skip(1).chain(closing)) {
        out.curve_to(
            plus(from.point, from.leaving),
            plus(to.point, to.arriving),
            p(to.point),
        );
    }
    if shape.closed() {
        out.close_path();
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/shape.rs"]
mod tests;
