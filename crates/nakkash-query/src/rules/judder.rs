//! `judder`: a layer moves farther in one frame than the eye follows as
//! nakkash, so it is seen to jump, unless motion blur smooths the step.

use super::{Craft, Finding, Measured, Reads, Sample, judder_limit};
use crate::report::{Code, Measure, Severity, Unit};
use nakkash_format::key::MBLUR;
use nakkash_format::{Address, Layer};
use nakkash_scene::{CompGraph, Node, SceneGraph};

/// The shortest run of steps that judders, in seconds: the peak of an
/// ease, a frame or two, passes before the eye follows it.
const LASTING: f64 = 1.0 / 8.0;

/// A layer that moves on its own should not step farther than the limit in
/// one frame unless it is blurred.
pub(super) const JUDDER: Craft = Craft {
    code: Code::Judder,
    severity: Severity::Warning,
    reads: Reads::Step(step),
    lasting: LASTING,
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
fn fix(address: &Address, _: Option<&Layer>, node: &Node, comp: &CompGraph) -> Option<String> {
    (!node.mblur && comp.shutter.open()).then(|| format!("{address} {MBLUR}=true"))
}

#[cfg(test)]
#[path = "../../tests/unit/rules/judder.rs"]
mod tests;
