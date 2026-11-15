//! `judder`: a layer moves farther in one frame than the eye follows as
//! motion, so it is seen to jump, unless motion blur smooths the step.

use super::{Craft, Finding, Measured, Reads, Sample, judder_limit};
use crate::report::{Code, Measure, Severity, Unit};
use motion_format::Address;
use motion_format::key::MBLUR;
use motion_scene::{CompGraph, Node, SceneGraph};

/// A layer that moves on its own should not step farther than the limit in
/// one frame unless it is blurred.
pub(super) const JUDDER: Craft = Craft {
    code: Code::Judder,
    severity: Severity::Warning,
    reads: Reads::Step(step),
    measure: |f| Measure::Above {
        value: (f.value - super::EPSILON).ceil(),
        limit: f.limit,
        unit: Unit::PxPerFrame,
    },
    fix,
};

/// Only a step the layer takes on its own counts: what moves it is checked
/// on its own layer.
fn step(before: &Sample, now: &Sample, graph: &SceneGraph) -> Measured {
    if now.blurred || !now.moves_from(before) {
        return Measured::Skipped;
    }
    let limit = judder_limit(graph);
    let value = now.moved_from(before);
    match value > limit + super::EPSILON {
        true => Measured::Found(Finding { value, limit }),
        false => Measured::Passed,
    }
}

/// Turning motion blur on, where its composition's shutter is open.
fn fix(address: &Address, node: &Node, comp: &CompGraph) -> Option<String> {
    (!node.mblur && comp.shutter.open()).then(|| format!("{address} {MBLUR}=true"))
}

#[cfg(test)]
#[path = "../../tests/unit/rules/judder.rs"]
mod tests;
