//! Boolean operations on outlines: the area two outlines cover together,
//! one without the other, both, or one but not both.
//!
//! An outline covers the points it winds around, as a fill of the
//! `nonzero` rule paints it; an open subpath is closed by a straight line,
//! as a fill closes it. A result is made of simple closed contours that do
//! not cross: outer ones run clockwise on the screen and holes the other
//! way, so it covers the same area under either fill rule. Curves stay curves: the contours are cubic
//! pieces of the inputs, cut where they cross, within [`ACCURACY`].
//!
//! Shape modifiers that work on areas build on this: merging outlines
//! today, and any later one that needs a union or a difference.

use kurbo::{BezPath, PathEl, Shape};
use linesweeper::topology::{BinaryWindingNumber, Topology};

/// How far, in the outlines' own units, a result may stray from the exact
/// one: a thousandth of a pixel at scale 1.
pub(crate) const ACCURACY: f64 = 1e-3;

/// How two areas combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    /// What either covers.
    Union,
    /// What the first covers and the second does not.
    Difference,
    /// What both cover.
    Intersection,
    /// What one covers and the other does not.
    Xor,
}

impl Op {
    /// Whether a point is in the result, from whether each side covers it.
    fn keeps(self, a: bool, b: bool) -> bool {
        match self {
            Op::Union => a || b,
            Op::Difference => a && !b,
            Op::Intersection => a && b,
            Op::Xor => a != b,
        }
    }
}

/// The area `a` covers combined by `op` with the area `b` covers, as
/// simple contours. An outline with a coordinate that is not finite covers
/// nothing.
pub(crate) fn combine(a: &BezPath, b: &BezPath, op: Op) -> BezPath {
    let (a, b) = (finite(closed(a)), finite(closed(b)));
    let eps = accuracy(&a, &b);
    let Ok(topology) = Topology::<BinaryWindingNumber>::from_paths_binary(&a, &b, eps) else {
        unreachable!("every subpath was closed")
    };
    let contours = topology.contours(|w| op.keeps(w.shape_a != 0, w.shape_b != 0));
    let mut out = BezPath::new();
    for contour in contours.contours() {
        out.extend(contour.path.iter());
    }
    out
}

/// The area `path` covers, as simple contours: the outline with its
/// crossings resolved.
pub(crate) fn region(path: &BezPath) -> BezPath {
    combine(path, &BezPath::new(), Op::Union)
}

/// `path` with every open subpath closed, and without subpaths that have no
/// segment.
fn closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    // Whether a subpath is being written, and whether it has a segment yet.
    let (mut open, mut drawn) = (false, false);
    let mut pending = None;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if open && drawn {
                    out.close_path();
                }
                (open, drawn) = (true, false);
                pending = Some(p);
            }
            PathEl::ClosePath => {
                if open && drawn {
                    out.close_path();
                }
                (open, drawn) = (false, false);
            }
            segment => {
                if let Some(p) = pending.take() {
                    out.move_to(p);
                }
                out.push(segment);
                drawn = true;
            }
        }
    }
    if open && drawn {
        out.close_path();
    }
    out
}

/// `path`, or nothing if a coordinate of it is not finite.
fn finite(path: BezPath) -> BezPath {
    match path.is_finite() {
        true => path,
        false => BezPath::new(),
    }
}

/// The sweep's accuracy for two outlines: [`ACCURACY`], or coarser where
/// their coordinates are so large that floating point cannot do better.
fn accuracy(a: &BezPath, b: &BezPath) -> f64 {
    let bounds = [a, b]
        .into_iter()
        .filter(|p| !p.elements().is_empty())
        .map(|p| p.bounding_box())
        .reduce(|x, y| x.union(y));
    let reach = bounds.map_or(0.0, |r| {
        r.min_x()
            .abs()
            .max(r.max_x().abs())
            .max(r.min_y().abs())
            .max(r.max_y().abs())
    });
    ACCURACY.max(reach * f64::EPSILON * 64.0)
}

#[cfg(test)]
#[path = "../tests/unit/boolean.rs"]
mod tests;
