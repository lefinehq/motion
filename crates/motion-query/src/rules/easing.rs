//! `easing`: a move that runs at one speed from start to end. Eased motion
//! is easier to follow than constant speed (Dragicevic et al. 2011), so a
//! move's fastest step should be well above its mean.

use super::{Craft, Finding, Measured, Move, Reads, Sample, px};
use crate::report::{Code, Measure, Severity, Unit};
use motion_format::{Address, Layer};
use motion_scene::{CompGraph, Node, SceneGraph};

/// The least a move's peak speed over its mean speed may be: constant speed
/// is 1.
const LEAST_PEAK: f64 = 1.4;

/// A move shorter than this many steps is too brief to judge.
const SHORTEST: u32 = 6;

/// A move slower than this many px a frame on average (on a 1080-px-tall
/// canvas) is a drift, which runs at one speed on purpose.
const DRIFT: f64 = 2.0;

/// A move should ease rather than run at one speed.
pub(super) const EASING: Craft = Craft {
    code: Code::Easing,
    severity: Severity::Warning,
    reads: Reads::Moves { speed, judge },
    lasting: 0.0,
    // Two decimals, rounded towards the limit's side it misses.
    measure: |f| Measure::Below {
        value: (f.value * 100.0 + super::EPSILON).floor() / 100.0,
        limit: f.limit,
        unit: Unit::Number,
    },
    fix,
};

/// How far its centre moves, when it moves on its own: turning or growing in
/// place is not a move across the frame.
fn speed(before: &Sample, now: &Sample) -> Option<f64> {
    if !(before.shown && now.shown && now.moves_from(before)) {
        return None;
    }
    let travel = now.centre().distance(before.centre());
    (travel > super::EPSILON).then_some(travel)
}

fn judge(found: &Move, graph: &SceneGraph) -> Measured {
    if found.steps() < SHORTEST || found.mean() < DRIFT * px(graph) {
        return Measured::Skipped;
    }
    let value = found.peak / found.mean();
    match value < LEAST_PEAK - super::EPSILON {
        true => Measured::Found(Finding {
            value,
            limit: LEAST_PEAK,
        }),
        false => Measured::Passed,
    }
}

/// A curve belongs on the track or entrance that drives the move, which one
/// edit of its written form cannot reach yet.
fn fix(_: &Address, _: Option<&Layer>, _: &Node, _: &CompGraph) -> Option<String> {
    None
}

#[cfg(test)]
#[path = "../../tests/unit/rules/easing.rs"]
mod tests;
