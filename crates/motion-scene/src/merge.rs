//! Merge: outlines replaced by one outline that combines them.
//!
//! A merge takes every outline listed before it in its group, those inside
//! earlier groups included, seen through those groups' transforms, in list
//! order. `merge` lays them one after another as they are; the other modes
//! combine the areas they cover, each read by the nonzero rule: the first
//! outline's area with the next one's, that result with the one after, and
//! so on. The outlines it takes are gone from the paints listed before it,
//! which draw nothing of them; the paints after it draw the one it leaves.

use kurbo::{Affine, BezPath};

use crate::boolean::{self, Op};
use crate::shape::Reach;
use crate::state::ItemState;

/// Replaces the outlines the merge `item` reaches by the one outline it
/// makes of them, added in its group's space.
pub(crate) fn apply(item: &ItemState, reach: Reach<'_>) {
    let Reach { above, outlines } = reach;
    if above.is_empty() {
        return;
    }
    let taken: Vec<BezPath> = above
        .iter()
        .map(|&(at, into)| into * std::mem::take(&mut outlines[at]))
        .collect();
    let merged = match op(item.setting("mode")) {
        None => taken.into_iter().flatten().collect(),
        Some(op) => {
            let mut parts = taken.iter();
            let first = parts.next().expect("at least one outline");
            parts.fold(boolean::region(first), |area, next| {
                boolean::combine(&area, next, op)
            })
        }
    };
    above.clear();
    above.push((outlines.len(), Affine::IDENTITY));
    outlines.push(merged);
}

/// The operation a mode combines areas by; `None` for `merge`, which keeps
/// the outlines as they are.
fn op(mode: &str) -> Option<Op> {
    match mode {
        "add" => Some(Op::Union),
        "subtract" => Some(Op::Difference),
        "intersect" => Some(Op::Intersection),
        "exclude" => Some(Op::Xor),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/merge.rs"]
mod tests;
