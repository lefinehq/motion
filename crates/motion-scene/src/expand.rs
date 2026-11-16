//! Expand: moving outlines out, or in, by an amount.
//!
//! Every side of an outline moves the same distance along its normal: to the
//! left of the direction it runs in, which is outward for an outline that
//! runs clockwise, as rectangles, ellipses and stars do by default. Where an
//! outline turns away from the side it moves to, the moved sides part, and a
//! join closes the gap: a miter, cut to a bevel past its limit, an arc, or a
//! bevel. Where it turns towards that side, the moved sides cross, and both
//! are cut where they cross. An open outline is moved along both of its
//! sides, out and back, and joined at its ends, so it becomes one closed
//! outline around the line.
//!
//! Copies repeat this one amount further out each: copy `k`, from 1, moves
//! the outline `amount · (k + copy-offset)`.

use kurbo::{Arc, BezPath, CubicBez, Line, ParamCurve, PathEl, PathSeg, Point, Vec2};
use motion_format::item::props;

use crate::state::ItemState;

/// How closely a moved curve follows the true parallel of its side, in the
/// units of the outline's space.
const TOLERANCE: f64 = 1e-3;

/// Below this, two points are the same and a length is none.
const EPSILON: f64 = 1e-9;

/// How deep the search for where two moved sides cross splits them.
const CROSSING_DEPTH: u32 = 40;

/// Moves the outlines at `at` in `outlines` by the expansion `item`.
pub(crate) fn apply(item: &ItemState, at: &[usize], outlines: &mut [BezPath]) {
    let amount = item.number(props::EXPAND_AMOUNT);
    let copies = item.number(props::EXPAND_COPIES).floor().max(0.0) as usize;
    let shift = item.number(props::EXPAND_COPY_OFFSET);
    let join = Join::of(item);
    for &i in at {
        let source = std::mem::take(&mut outlines[i]);
        let mut out = BezPath::new();
        for k in 1..=copies {
            out.extend(&expand(&source, amount * (k as f64 + shift), join));
        }
        outlines[i] = out;
    }
}

/// How moved sides that part are joined.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Join {
    /// Extended until they meet, unless that is further from the corner
    /// than `limit` amounts; then as a bevel.
    Miter {
        /// The furthest a miter reaches from its corner, in amounts.
        limit: f64,
    },
    /// By an arc about the corner.
    Round,
    /// By a straight line.
    Bevel,
}

impl Join {
    /// The join of the expansion `item`.
    fn of(item: &ItemState) -> Join {
        match item.setting("join") {
            "round" => Join::Round,
            "bevel" => Join::Bevel,
            _ => Join::Miter {
                limit: item.number(props::EXPAND_MITER),
            },
        }
    }
}

/// `path` with every subpath moved `amount` to the left of the way it runs.
pub(crate) fn expand(path: &BezPath, amount: f64, join: Join) -> BezPath {
    if amount == 0.0 {
        return path.clone();
    }
    let mut out = BezPath::new();
    for (sides, closed) in subpaths(path) {
        let loop_sides = match closed {
            true => sides,
            // Out along one side and back along the other.
            false => sides
                .iter()
                .copied()
                .chain(sides.iter().rev().map(PathSeg::reverse))
                .collect(),
        };
        if let Some(moved) = expand_loop(&loop_sides, amount, join) {
            out.extend(&moved);
        }
    }
    out
}

/// Each subpath of `path` as its sides, those of no length left out, and
/// whether it is closed. A closed subpath's sides include the one that
/// closes it.
fn subpaths(path: &BezPath) -> Vec<(Vec<PathSeg>, bool)> {
    let mut all = Vec::new();
    let mut sides = Vec::new();
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    let mut push = |sides: &mut Vec<PathSeg>, closed: bool| {
        if !sides.is_empty() {
            all.push((std::mem::take(sides), closed));
        }
    };
    for el in path.elements() {
        let seg = match *el {
            PathEl::MoveTo(p) => {
                push(&mut sides, false);
                (start, last) = (p, p);
                continue;
            }
            PathEl::LineTo(p) => PathSeg::Line(Line::new(last, p)),
            PathEl::QuadTo(p1, p2) => PathSeg::Cubic(kurbo::QuadBez::new(last, p1, p2).raise()),
            PathEl::CurveTo(p1, p2, p3) => PathSeg::Cubic(CubicBez::new(last, p1, p2, p3)),
            PathEl::ClosePath => {
                sides.extend(straight(Line::new(last, start)));
                push(&mut sides, true);
                last = start;
                continue;
            }
        };
        last = seg.end();
        sides.extend(straight(seg));
    }
    push(&mut sides, false);
    all
}

/// `seg` as a line when it runs straight, and nothing when it has no
/// length. A cubic whose handles lie on the line between its ends, between
/// them, runs straight.
fn straight(seg: impl Into<PathSeg>) -> Option<PathSeg> {
    let seg = seg.into();
    let (p0, p3) = (seg.start(), seg.end());
    let chord = p3 - p0;
    let length = chord.hypot();
    let PathSeg::Cubic(c) = seg else {
        return (length > EPSILON).then_some(seg);
    };
    if length <= EPSILON {
        let still = [c.p1, c.p2].iter().all(|&p| (p - p0).hypot() <= EPSILON);
        return (!still).then_some(seg);
    }
    let on_chord = |p: Point| {
        let v = p - p0;
        v.cross(chord).abs() <= EPSILON * length && (0.0..=length * length).contains(&v.dot(chord))
    };
    match on_chord(c.p1) && on_chord(c.p2) {
        true => Some(PathSeg::Line(Line::new(p0, p3))),
        false => Some(seg),
    }
}

/// The unit tangents at the start and the end of `seg`, which skip a handle
/// that sits on its end.
fn tangents(seg: &PathSeg) -> (Vec2, Vec2) {
    let unit = |v: Vec2| v.normalize();
    match *seg {
        PathSeg::Line(l) => {
            let t = unit(l.p1 - l.p0);
            (t, t)
        }
        PathSeg::Quad(q) => tangents(&PathSeg::Cubic(q.raise())),
        PathSeg::Cubic(c) => {
            let first = [c.p1, c.p2, c.p3]
                .into_iter()
                .find(|&p| (p - c.p0).hypot() > EPSILON)
                .unwrap_or(c.p3);
            let last = [c.p2, c.p1, c.p0]
                .into_iter()
                .find(|&p| (c.p3 - p).hypot() > EPSILON)
                .unwrap_or(c.p0);
            (unit(first - c.p0), unit(c.p3 - last))
        }
    }
}

/// The side to the left of a unit tangent, in a space whose y runs down: up
/// for a tangent to the right.
fn left(tangent: Vec2) -> Vec2 {
    Vec2::new(tangent.y, -tangent.x)
}

/// One side moved `amount` to its left, as pieces in order.
fn move_side(seg: &PathSeg, amount: f64) -> Vec<PathSeg> {
    match *seg {
        PathSeg::Line(l) => {
            let by = left(tangents(seg).0) * amount;
            vec![PathSeg::Line(Line::new(l.p0 + by, l.p1 + by))]
        }
        PathSeg::Quad(q) => move_side(&PathSeg::Cubic(q.raise()), amount),
        PathSeg::Cubic(c) => {
            let mut out = BezPath::new();
            // The parallel's offset is to the right of the way a curve runs.
            kurbo::offset::offset_cubic(c, -amount, TOLERANCE, &mut out);
            out.segments().collect()
        }
    }
}

/// A closed loop of `sides`, moved `amount` and joined; `None` if it has no
/// sides.
fn expand_loop(sides: &[PathSeg], amount: f64, join: Join) -> Option<BezPath> {
    if sides.is_empty() {
        return None;
    }
    let mut moved: Vec<Vec<PathSeg>> = sides.iter().map(|s| move_side(s, amount)).collect();
    let n = sides.len();
    // The joint after each side: cut both where they cross, or the pieces
    // that join them where they part.
    let mut joins: Vec<Vec<PathSeg>> = vec![Vec::new(); n];
    for j in 0..n {
        let next = (j + 1) % n;
        let corner = sides[j].end();
        let (_, arriving) = tangents(&sides[j]);
        let (leaving, _) = tangents(&sides[next]);
        let (Some(end), Some(start)) = (
            moved[j].last().map(PathSeg::end),
            moved[next].first().map(PathSeg::start),
        ) else {
            continue;
        };
        if (end - start).hypot() <= TOLERANCE {
            continue;
        }
        let turn = arriving.cross(leaving);
        let parting = turn * amount > 0.0 || (turn.abs() <= EPSILON && arriving.dot(leaving) < 0.0);
        if !parting {
            if cut_where_crossing(&mut moved, j, next) {
                continue;
            }
            // Sides that turn in but do not cross go through the corner.
            joins[j] = vec![
                PathSeg::Line(Line::new(end, corner)),
                PathSeg::Line(Line::new(corner, start)),
            ];
            continue;
        }
        joins[j] = join_parted(end, arriving, start, leaving, corner, amount, join);
    }
    let mut out = BezPath::new();
    let first = moved.iter().flatten().next().map(PathSeg::start)?;
    out.move_to(first);
    for (pieces, joint) in moved.iter().zip(&joins) {
        for piece in pieces.iter().chain(joint) {
            out.push(piece.as_path_el());
        }
    }
    out.close_path();
    Some(out)
}

/// The pieces that join two moved sides that part at `corner`: the first
/// ends at `end`, running along `arriving`, and the next starts at `start`,
/// running along `leaving`.
fn join_parted(
    end: Point,
    arriving: Vec2,
    start: Point,
    leaving: Vec2,
    corner: Point,
    amount: f64,
    join: Join,
) -> Vec<PathSeg> {
    let bevel = vec![PathSeg::Line(Line::new(end, start))];
    match join {
        Join::Bevel => bevel,
        Join::Miter { limit } => {
            let Some(tip) = meet(end, arriving, start, leaving) else {
                return bevel;
            };
            // A miter that points back, behind both sides, does not meet.
            if (tip - end).dot(arriving) < 0.0 || (start - tip).dot(leaving) < 0.0 {
                return bevel;
            }
            match (tip - corner).hypot() <= limit * amount.abs() {
                true => vec![
                    PathSeg::Line(Line::new(end, tip)),
                    PathSeg::Line(Line::new(tip, start)),
                ],
                false => bevel,
            }
        }
        Join::Round => {
            let (from, to) = (end - corner, start - corner);
            let mut sweep = from.cross(to).atan2(from.dot(to));
            // The arc goes round the outside of the turn, the way the first
            // side was running.
            let middle = Vec2::from_angle(from.atan2() + sweep / 2.0);
            if middle.dot(arriving - leaving) < 0.0 {
                sweep -= sweep.signum() * std::f64::consts::TAU;
            }
            let arc = Arc::new(
                corner,
                (amount.abs(), amount.abs()),
                from.atan2(),
                sweep,
                0.0,
            );
            let mut pieces = Vec::new();
            let mut at = end;
            for el in arc.append_iter(TOLERANCE) {
                if let PathEl::CurveTo(p1, p2, p3) = el {
                    pieces.push(PathSeg::Cubic(CubicBez::new(at, p1, p2, p3)));
                    at = p3;
                }
            }
            if let Some(PathSeg::Cubic(last)) = pieces.last_mut() {
                last.p3 = start;
            }
            pieces
        }
    }
}

/// Where the line through `a` along `u` meets the line through `b` along
/// `v`, if they are not parallel.
fn meet(a: Point, u: Vec2, b: Point, v: Vec2) -> Option<Point> {
    let det = u.cross(v);
    if det.abs() <= EPSILON {
        return None;
    }
    let s = (b - a).cross(v) / det;
    Some(a + u * s)
}

/// Cuts the moved side `j` and the moved side `next` after it where they
/// cross nearest their joint: the first ends there and the next starts
/// there. False if they do not cross.
fn cut_where_crossing(moved: &mut [Vec<PathSeg>], j: usize, next: usize) -> bool {
    let (before, after) = (moved[j].clone(), moved[next].clone());
    for (ia, a) in before.iter().enumerate().rev() {
        for (ib, b) in after.iter().enumerate() {
            let Some((ta, tb)) = crossing(a, b) else {
                continue;
            };
            let mut kept_before: Vec<PathSeg> = before[..ia].to_vec();
            kept_before.push(a.subsegment(0.0..ta));
            let mut kept_after = vec![b.subsegment(tb..1.0)];
            kept_after.extend_from_slice(&after[ib + 1..]);
            // Both ends meet at one point.
            let at = kept_before.last().map(PathSeg::end).expect("a piece");
            set_start(&mut kept_after[0], at);
            moved[j] = kept_before;
            moved[next] = kept_after;
            return true;
        }
    }
    false
}

/// Moves the start of `seg` to `at`.
fn set_start(seg: &mut PathSeg, at: Point) {
    match seg {
        PathSeg::Line(l) => l.p0 = at,
        PathSeg::Quad(q) => q.p0 = at,
        PathSeg::Cubic(c) => c.p0 = at,
    }
}

/// Where `a` and `b` cross, as their parameters: the crossing latest along
/// `a`, and of those the earliest along `b`. A crossing at the very end of
/// `a` and start of `b`, where they join, does not count.
fn crossing(a: &PathSeg, b: &PathSeg) -> Option<(f64, f64)> {
    let mut found = Vec::new();
    split_crossing(a, 0.0..1.0, b, 0.0..1.0, 0, &mut found);
    found
        .into_iter()
        .filter(|&(ta, tb)| !(ta >= 1.0 - 1e-6 && tb <= 1e-6))
        .max_by(|x, y| x.0.total_cmp(&y.0).then(y.1.total_cmp(&x.1)))
}

/// Finds where the parts `ra` of `a` and `rb` of `b` cross by splitting
/// both while their boxes overlap.
fn split_crossing(
    a: &PathSeg,
    ra: std::ops::Range<f64>,
    b: &PathSeg,
    rb: std::ops::Range<f64>,
    depth: u32,
    found: &mut Vec<(f64, f64)>,
) {
    use kurbo::ParamCurveExtrema;
    let (pa, pb) = (a.subsegment(ra.clone()), b.subsegment(rb.clone()));
    let (box_a, box_b) = (pa.bounding_box(), pb.bounding_box());
    if box_a.x1 < box_b.x0 || box_b.x1 < box_a.x0 || box_a.y1 < box_b.y0 || box_b.y1 < box_a.y0 {
        return;
    }
    let small = |r: kurbo::Rect| r.width().max(r.height()) <= TOLERANCE * 1e-3;
    if depth >= CROSSING_DEPTH || (small(box_a) && small(box_b)) {
        let (ta, tb) = ((ra.start + ra.end) / 2.0, (rb.start + rb.end) / 2.0);
        if !found
            .iter()
            .any(|&(x, y)| (x - ta).abs() < 1e-6 && (y - tb).abs() < 1e-6)
        {
            found.push((ta, tb));
        }
        return;
    }
    // Two straight pieces cross at one point, if at all.
    if let (PathSeg::Line(la), PathSeg::Line(lb)) = (pa, pb) {
        let (u, v) = (la.p1 - la.p0, lb.p1 - lb.p0);
        let det = u.cross(v);
        if det.abs() > EPSILON {
            let s = (lb.p0 - la.p0).cross(v) / det;
            let t = (lb.p0 - la.p0).cross(u) / det;
            if (0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t) {
                let ta = ra.start + (ra.end - ra.start) * s;
                let tb = rb.start + (rb.end - rb.start) * t;
                found.push((ta, tb));
            }
        }
        return;
    }
    let (ma, mb) = ((ra.start + ra.end) / 2.0, (rb.start + rb.end) / 2.0);
    for sa in [ra.start..ma, ma..ra.end] {
        for sb in [rb.start..mb, mb..rb.end] {
            split_crossing(a, sa.clone(), b, sb, depth + 1, found);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/expand.rs"]
mod tests;
