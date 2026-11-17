//! `crowd`: more layers move on their own at once than people track. People
//! follow about four moving objects at a time (multiple-object tracking).

use super::{Finding, Sample, px};
use crate::report::{Measure, Unit};
use nakkash_scene::SceneGraph;

/// The most layers that may move on their own at once.
pub(crate) const MOST: f64 = 4.0;

/// Below this many px a frame (on a 1080-px-tall canvas), a layer drifts
/// rather than draws the eye.
const NOTICED: f64 = 2.0;

/// Whether the layer catches the eye over the step: it moves on its own,
/// fast enough to notice.
pub(crate) fn catches_the_eye(before: &Sample, now: &Sample, graph: &SceneGraph) -> bool {
    before.shown
        && now.shown
        && now.moves_from(before)
        && now.moved_from(before) > NOTICED * px(graph)
}

/// The finding for a frame where `movers` layers move at once.
pub(crate) fn judge(movers: usize) -> Option<Finding> {
    let value = movers as f64;
    (value > MOST).then_some(Finding { value, limit: MOST })
}

/// How a crowd prints: the most movers at once against the limit.
pub(crate) fn measure(f: Finding) -> Measure {
    Measure::Above {
        value: f.value,
        limit: f.limit,
        unit: Unit::Number,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/rules/crowd.rs"]
mod tests;
