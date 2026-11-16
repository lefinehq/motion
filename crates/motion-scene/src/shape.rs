//! Shape items resolved into what a renderer draws: each paint with the
//! outlines it covers, back to front.
//!
//! Within a group the first item is on top. A paint draws every outline
//! listed before it in its group, those inside earlier groups included,
//! through those groups' transforms. It strokes and fills in its own group's
//! space and takes the opacity of every group around it.
//!
//! A modifier changes the outlines listed before it the same way, in place:
//! every paint that draws them draws them changed, wherever it sits. Inner
//! groups' modifiers act before those of the groups around them. A merge
//! replaces them by one outline of its own, so the paints before it draw
//! nothing of them.

use kurbo::{Affine, BezPath, Point, Rect, Shape, Vec2};
use motion_format::item::{Role, props};
use motion_format::property::GROUP;

use crate::evaluate::local_transform;
use crate::state::ItemState;
use crate::{merge, round, trim};

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
const OUTLINES: [(&str, Draw); 4] = [
    ("rect", rect),
    ("ellipse", ellipse),
    ("star", star),
    ("path", path),
];

/// What a modifier reaches in the walk.
pub(crate) struct Reach<'a> {
    /// The outlines listed before it, each as its index in `outlines` and
    /// its transform into the modifier's group space. A modifier that
    /// replaces them leaves the new ones here.
    pub(crate) above: &'a mut Vec<(usize, Affine)>,
    /// Every outline of the layer so far, each in the space of the group it
    /// sits in. A modifier may add outlines.
    pub(crate) outlines: &'a mut Vec<BezPath>,
}

/// Changes the outlines a modifier reaches.
type Modify = fn(&ItemState, Reach<'_>);

/// What each modifier kind does, by name; every modifier kind has a row.
const MODIFIERS: [(&str, Modify); 3] = [
    ("trim", trim_outlines),
    ("merge", merge::apply),
    ("round-corners", round_outlines),
];

/// Cuts the outlines a trim reaches, each in its own space.
fn trim_outlines(item: &ItemState, reach: Reach<'_>) {
    let at: Vec<usize> = reach.above.iter().map(|&(at, _)| at).collect();
    trim::apply(item, &at, reach.outlines);
}

/// Rounds the corners of the outlines round corners reach, each in its own
/// space.
fn round_outlines(item: &ItemState, reach: Reach<'_>) {
    let at: Vec<usize> = reach.above.iter().map(|&(at, _)| at).collect();
    round::apply(item, &at, reach.outlines);
}

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
    let mut walk = Walk::default();
    walk.group(items, Affine::IDENTITY, 1.0);
    // Paints draw the outlines as every modifier left them, so they are
    // built only once the walk is done.
    let mut draws: Vec<ShapeDraw> = walk
        .paints
        .into_iter()
        .filter_map(|paint| {
            let mut outline = BezPath::new();
            for &(at, into) in &paint.covers {
                outline.extend(into * &walk.outlines[at]);
            }
            (!outline.elements().is_empty()).then_some(ShapeDraw {
                paint: paint.paint,
                outline,
                transform: paint.transform,
                opacity: paint.opacity,
            })
        })
        .collect();
    draws.reverse();
    draws
}

/// The outlines and paints of a layer's items, gathered in one walk.
#[derive(Default)]
struct Walk {
    /// Every outline, each in the space of the group it sits in.
    outlines: Vec<BezPath>,
    /// Every paint, top first.
    paints: Vec<Paint>,
}

/// A paint and the outlines it covers.
struct Paint {
    /// The fill or stroke, without items.
    paint: ItemState,
    /// Each outline it covers and the transform from that outline's space
    /// into the paint's.
    covers: Vec<(usize, Affine)>,
    /// From the paint's group space to the layer's.
    transform: Affine,
    /// The product of the opacities of the groups around the paint.
    opacity: f64,
}

impl Walk {
    /// Walks one group's `items` in order and returns the outlines they hold,
    /// with each one's transform into the group's space: `transform` takes
    /// the group's space to the layer's and `opacity` is the groups' product.
    fn group(
        &mut self,
        items: &[ItemState],
        transform: Affine,
        opacity: f64,
    ) -> Vec<(usize, Affine)> {
        let mut above: Vec<(usize, Affine)> = Vec::new();
        for item in items {
            match item.kind.role {
                Role::Geometry => {
                    above.push((self.outlines.len(), Affine::IDENTITY));
                    self.outlines.push(outline(item).expect("a geometry item"));
                }
                Role::Group => {
                    let (own, own_opacity) = group_transform(item);
                    let inner = self.group(&item.items, transform * own, opacity * own_opacity);
                    above.extend(inner.into_iter().map(|(at, into)| (at, own * into)));
                }
                Role::Modifier => {
                    let (_, modify) = MODIFIERS
                        .iter()
                        .find(|(name, _)| *name == item.kind.name)
                        .expect("every modifier kind has a row");
                    modify(
                        item,
                        Reach {
                            above: &mut above,
                            outlines: &mut self.outlines,
                        },
                    );
                }
                Role::Paint if !above.is_empty() => self.paints.push(Paint {
                    paint: ItemState {
                        items: Vec::new(),
                        ..item.clone()
                    },
                    covers: above.clone(),
                    transform,
                    opacity,
                }),
                Role::Paint | Role::Effect | Role::Animator | Role::Selector | Role::Mask => {}
            }
        }
        above
    }
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

/// A star or a regular polygon about its centre: its corners on two circles,
/// or on one for a polygon, at equal angles from the first point, straight
/// up from the centre when `rotate` is 0, and on clockwise, or the other
/// way round for `counterclockwise`. A star's points alternate with its
/// inner corners. `points` counts by its whole part.
///
/// A corner's handles run along the circle it is on, both ways, each as
/// long as its roundness times a quarter of the arc between two points of
/// that circle: `2π r / (4 n)` for `n` points. A corner of roundness 0 is
/// sharp, and the sides between sharp corners are straight lines.
fn star(item: &ItemState) -> BezPath {
    let [cx, cy] = item.point(props::STAR_POSITION);
    let c = Point::new(cx, cy);
    let n = item.number(props::STAR_POINTS).floor().max(0.0);
    let turn = match item.setting("direction") {
        "counterclockwise" => -1.0,
        _ => 1.0,
    };
    let outer = (
        item.number(props::STAR_OUTER_RADIUS),
        item.number(props::STAR_OUTER_ROUNDNESS),
    );
    // A corner and its radius and roundness, in order around the shape.
    let corners: Vec<(f64, f64)> = match item.setting("type") {
        "polygon" => vec![outer; n as usize],
        _ => {
            let inner = (
                item.number(props::STAR_INNER_RADIUS),
                item.number(props::STAR_INNER_ROUNDNESS),
            );
            [outer, inner].repeat(n as usize)
        }
    };
    let mut path = BezPath::new();
    if corners.is_empty() {
        return path;
    }
    let step = std::f64::consts::TAU / corners.len() as f64 * turn;
    let first = (item.number(props::STAR_ROTATE) - 90.0).to_radians();
    // Each corner's point and its handle forward along the outline.
    let at: Vec<(Point, Vec2)> = corners
        .iter()
        .enumerate()
        .map(|(i, &(radius, roundness))| {
            let angle = first + step * i as f64;
            let out = Vec2::from_angle(angle);
            let along = Vec2::new(-out.y, out.x) * turn;
            let reach = radius * std::f64::consts::TAU / (4.0 * n) * roundness;
            (c + out * radius, along * reach)
        })
        .collect();
    path.move_to(at[0].0);
    for (i, &(from, leaving)) in at.iter().enumerate() {
        let (to, arriving) = at[(i + 1) % at.len()];
        match leaving == Vec2::ZERO && arriving == Vec2::ZERO {
            true => path.line_to(to),
            false => path.curve_to(from + leaving, to - arriving, to),
        }
    }
    path.close_path();
    path
}

/// A free path's outline, as [`bezier`] draws it.
fn path(item: &ItemState) -> BezPath {
    bezier(item.path(props::PATH_PATH))
}

/// A scene path as an outline: each vertex to the next through its leaving
/// handle and the next one's arriving handle, and back to the first when
/// closed.
pub fn bezier(shape: &motion_format::Path) -> BezPath {
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
