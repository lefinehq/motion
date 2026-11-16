//! Repeater: the copies of what lies above it in its group.
//!
//! Copy `k` of a repeater stands `t = k + offset` steps along: moved by `t`
//! times `position`, turned by `t` times `rotate` and scaled by `scale` to
//! the power `t`, both about `anchor`. The steps do not compound: a copy's
//! move is not turned or scaled by the copies before it. Its opacity runs
//! from `start-opacity` towards `end-opacity` in equal steps of a
//! `copies`-th of the way, so the last copy stops one step short of
//! `end-opacity`. A fractional count draws one more copy, faded by the
//! fraction.

use kurbo::{Affine, Vec2};
use motion_format::item::props;

use crate::state::ItemState;

/// One copy a repeater draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Copy {
    /// Its number, 0 for the first.
    pub index: usize,
    /// From the copy's space into its group's.
    pub transform: Affine,
    /// The opacity its paints take.
    pub opacity: f64,
}

/// The copies the repeater `item` draws, top first: the first copy on top
/// with its `composite` `below`, the last with `above`.
pub(crate) fn copies(item: &ItemState) -> Vec<Copy> {
    let count = item.number(props::REPEATER_COPIES).max(0.0);
    let whole = count.ceil();
    // The copy past the whole count shows as much of itself as the count's
    // fraction.
    let part = match count - count.floor() {
        0.0 => 1.0,
        fraction => fraction,
    };
    let offset = item.number(props::REPEATER_OFFSET);
    let (start, end) = (
        item.number(props::REPEATER_START_OPACITY),
        item.number(props::REPEATER_END_OPACITY),
    );
    let n = whole as usize;
    let copy = |index: usize| {
        let last = index + 1 == n;
        let fade = start + (end - start) * index as f64 / count;
        Copy {
            index,
            transform: step(item, index as f64 + offset),
            opacity: fade * if last { part } else { 1.0 },
        }
    };
    match item.setting("composite") == "above" {
        true => (0..n).rev().map(copy).collect(),
        false => (0..n).map(copy).collect(),
    }
}

/// The transform `t` steps along: `t` moves, turns and scalings, the last
/// two about the anchor.
fn step(item: &ItemState, t: f64) -> Affine {
    let [px, py] = item.point(props::REPEATER_POSITION);
    let [ax, ay] = item.point(props::REPEATER_ANCHOR);
    let [sx, sy] = item.point(props::REPEATER_SCALE);
    let anchor = Vec2::new(ax, ay);
    Affine::translate(Vec2::new(px, py) * t + anchor)
        * Affine::rotate((item.number(props::REPEATER_ROTATE) * t).to_radians())
        * Affine::scale_non_uniform(power(sx, t), power(sy, t))
        * Affine::translate(-anchor)
}

/// `factor` to the power `t`. A negative factor mirrors every other whole
/// step, and its size follows its magnitude's power.
fn power(factor: f64, t: f64) -> f64 {
    let size = factor.abs().powf(t);
    match factor < 0.0 && t.round().rem_euclid(2.0) == 1.0 {
        true => -size,
        false => size,
    }
}

#[cfg(test)]
#[path = "../tests/unit/repeater.rs"]
mod tests;
