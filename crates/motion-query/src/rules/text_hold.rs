//! `text_hold`: a text layer stays readable for less time than reading it
//! takes, at 20 characters a second and never under 5/6 s, as streaming
//! subtitle guidelines ask.

use super::{Craft, Finding, Reads, Sample, judder_limit};
use crate::report::{Code, Measure, Severity, Unit};
use motion_format::Address;
use motion_scene::{CompGraph, Node, SceneGraph};

/// Characters read in a second.
const CHARS_PER_SECOND: f64 = 20.0;

/// The shortest time any text should stay readable, in seconds.
const SHORTEST: f64 = 5.0 / 6.0;

/// At least this opaque, text counts as readable.
const READABLE_OPACITY: f64 = 0.9;

/// A text layer should stay readable long enough to read.
pub(super) const TEXT_HOLD: Craft = Craft {
    code: Code::TextHold,
    severity: Severity::Warning,
    reads: Reads::Stretch { holds, judge },
    // Tenths of a second, rounded away from the limit so the gap shows.
    measure: |f| Measure::Below {
        value: (f.value * 10.0 + super::EPSILON).floor() / 10.0,
        limit: (f.limit * 10.0 - super::EPSILON).ceil() / 10.0,
        unit: Unit::Seconds,
    },
    fix,
};

/// Readable: a shown text, nearly opaque, not moving faster than the eye
/// follows.
fn holds(now: &Sample, before: Option<&Sample>, graph: &SceneGraph) -> bool {
    let still = before
        .filter(|b| b.shown)
        .is_none_or(|b| now.moved_from(b) <= judder_limit(graph));
    now.chars.is_some() && now.shown && now.opacity >= READABLE_OPACITY && still
}

/// Too short for its characters.
fn judge(frames: u32, first: &Sample, graph: &SceneGraph) -> Option<Finding> {
    let chars = first.chars?;
    let limit = (chars as f64 / CHARS_PER_SECOND).max(SHORTEST);
    let value = f64::from(frames) / graph.root.fps;
    (value < limit - super::EPSILON).then_some(Finding { value, limit })
}

/// Which span would hold it long enough depends on what follows it, so no
/// single edit is offered.
fn fix(_: &Address, _: &Node, _: &CompGraph) -> Option<String> {
    None
}

#[cfg(test)]
#[path = "../../tests/unit/rules/text_hold.rs"]
mod tests;
