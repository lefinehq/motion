//! Round corners: sharp corners of outlines replaced by curves.
//!
//! A corner is a vertex with no handle on either side. Each one is cut back
//! along both of its sides, by the radius or by half the way to the vertex
//! at the other end of that side if that is shorter, and the two cut points
//! are joined by a cubic whose handles reach from each towards the corner,
//! `0.5519` of the way: a quarter circle where the sides meet square and
//! the cuts are equal. Vertices with a handle stay as they are, and so do
//! the two ends of an open subpath. Lengths are in the space each outline
//! sits in.

use kurbo::{BezPath, PathEl, Point, Vec2};
use motion_format::item::props;

use crate::state::ItemState;

/// How far towards the corner a rounded corner's handles reach, as a share
/// of the cut: close to the `4/3 (√2 − 1)` of a quarter circle.
const REACH: f64 = 0.5519;

/// Rounds the corners of each outline at `at` in `outlines` by the
/// round-corners `item`'s radius.
pub(crate) fn apply(item: &ItemState, at: &[usize], outlines: &mut [BezPath]) {
    let radius = item.number(props::ROUND_CORNERS_RADIUS);
    if radius <= 0.0 {
        return;
    }
    for &i in at {
        outlines[i] = round(&outlines[i], radius);
    }
}

/// One vertex of a subpath and its handles, as points.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Vertex {
    /// The handle the curve arrives along.
    arriving: Point,
    /// The vertex.
    point: Point,
    /// The handle the curve leaves along.
    leaving: Point,
}

impl Vertex {
    fn sharp(point: Point) -> Vertex {
        Vertex {
            arriving: point,
            point,
            leaving: point,
        }
    }

    fn is_corner(self) -> bool {
        self.arriving == self.point && self.leaving == self.point
    }
}

/// A subpath as its vertices, and whether it closes.
struct Subpath {
    vertices: Vec<Vertex>,
    closed: bool,
}

/// `path` with its corners rounded by `radius`.
fn round(path: &BezPath, radius: f64) -> BezPath {
    let mut out = BezPath::new();
    for sub in subpaths(path) {
        let n = sub.vertices.len();
        let mut vertices = Vec::with_capacity(n * 2);
        for (i, &v) in sub.vertices.iter().enumerate() {
            let end = !sub.closed && (i == 0 || i == n - 1);
            if !v.is_corner() || end || n < 2 {
                vertices.push(v);
                continue;
            }
            let previous = sub.vertices[(i + n - 1) % n].point;
            let next = sub.vertices[(i + 1) % n].point;
            let toward = |other: Point| {
                let side: Vec2 = other - v.point;
                let length = side.hypot();
                match length > 0.0 {
                    true => v.point + side * (radius.min(length / 2.0) / length),
                    false => v.point,
                }
            };
            let (a, b) = (toward(previous), toward(next));
            vertices.push(Vertex {
                arriving: a,
                point: a,
                leaving: a + (v.point - a) * REACH,
            });
            vertices.push(Vertex {
                arriving: b + (v.point - b) * REACH,
                point: b,
                leaving: b,
            });
        }
        write(&mut out, &vertices, sub.closed);
    }
    out
}

/// The subpaths of `path` as vertices. A closing segment that ends where
/// the subpath started folds into its first vertex.
fn subpaths(path: &BezPath) -> Vec<Subpath> {
    let mut subs: Vec<Subpath> = Vec::new();
    for el in path.elements() {
        let vertices = subs.last_mut().map(|s| &mut s.vertices);
        match (*el, vertices) {
            (PathEl::MoveTo(p), _) => subs.push(Subpath {
                vertices: vec![Vertex::sharp(p)],
                closed: false,
            }),
            (PathEl::LineTo(p), Some(vertices)) => vertices.push(Vertex::sharp(p)),
            (PathEl::QuadTo(c, p), Some(vertices)) => {
                // The quadratic's cubic handles, two thirds of the way to its
                // control point.
                let last = vertices.last_mut().expect("a subpath starts with a vertex");
                last.leaving = last.point + (c - last.point) * (2.0 / 3.0);
                vertices.push(Vertex {
                    arriving: p + (c - p) * (2.0 / 3.0),
                    point: p,
                    leaving: p,
                });
            }
            (PathEl::CurveTo(c1, c2, p), Some(vertices)) => {
                vertices
                    .last_mut()
                    .expect("a subpath starts with a vertex")
                    .leaving = c1;
                vertices.push(Vertex {
                    arriving: c2,
                    point: p,
                    leaving: p,
                });
            }
            (PathEl::ClosePath, Some(_)) => {
                let sub = subs.last_mut().expect("a subpath");
                sub.closed = true;
                let n = sub.vertices.len();
                if n > 1 && sub.vertices[n - 1].point == sub.vertices[0].point {
                    let last = sub.vertices.pop().expect("more than one vertex");
                    sub.vertices[0].arriving = last.arriving;
                }
            }
            // A segment before any move has nowhere to start from.
            (_, None) => {}
        }
    }
    subs
}

/// Appends a subpath of `vertices` to `out`: straight where both handles of
/// a segment lie on its ends, curved otherwise.
fn write(out: &mut BezPath, vertices: &[Vertex], closed: bool) {
    let Some(first) = vertices.first() else {
        return;
    };
    out.move_to(first.point);
    let closing = closed.then_some(first);
    for (from, to) in vertices.iter().zip(vertices.iter().skip(1).chain(closing)) {
        match from.leaving == from.point && to.arriving == to.point {
            true => out.line_to(to.point),
            false => out.curve_to(from.leaving, to.arriving, to.point),
        }
    }
    if closed {
        out.close_path();
    }
}

#[cfg(test)]
#[path = "../tests/unit/round.rs"]
mod tests;
