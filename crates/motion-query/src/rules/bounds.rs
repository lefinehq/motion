//! Rules about a node's box leaving the canvas.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step): `offscreen` for the canvas itself, `unsafe` for the title-safe area
//! inside it. Both measure only the axes on which the box fits their area, so
//! a full-bleed background is never reported (see the module docs of
//! `rules`).

use super::{Area, Overshoot, Rule, overshoot, visible};
use crate::report::Severity;
use motion_eval::NodeState;
use motion_scene::SceneGraph;

/// The title-safe area keeps this fraction of the canvas free on each side.
const SAFE_INSET: f64 = 0.05;

/// A visible node must lie inside the canvas.
pub(super) const OFFSCREEN: Rule = Rule {
    code: "offscreen",
    severity: Severity::Error,
    suppressed_by: None,
    evaluate: offscreen,
    // Aim the fix at the safe area so one edit clears both rules; a sweep
    // that does not fit the safe area falls back to the canvas.
    areas: |graph| (canvas(graph), safe_area(graph)),
};

/// A visible node should lie inside the title-safe area.
pub(super) const UNSAFE: Rule = Rule {
    code: "unsafe",
    severity: Severity::Warning,
    // An off-screen node is also outside the safe area; say it once.
    suppressed_by: Some("offscreen"),
    evaluate: outside_safe_area,
    areas: |graph| (safe_area(graph), safe_area(graph)),
};

/// The whole canvas.
fn canvas(graph: &SceneGraph) -> Area {
    (0.0, 0.0, graph.width, graph.height)
}

/// The title-safe area of the canvas.
fn safe_area(graph: &SceneGraph) -> Area {
    let (dx, dy) = (graph.width * SAFE_INSET, graph.height * SAFE_INSET);
    (dx, dy, graph.width - dx, graph.height - dy)
}

fn offscreen(node: &NodeState, graph: &SceneGraph) -> Option<Overshoot> {
    // Invisible nodes (before their entrance) may start off-screen.
    if !visible(node) {
        return None;
    }
    overshoot(node, canvas(graph))
}

fn outside_safe_area(node: &NodeState, graph: &SceneGraph) -> Option<Overshoot> {
    if !visible(node) {
        return None;
    }
    overshoot(node, safe_area(graph))
}

#[cfg(test)]
#[path = "../../tests/unit/rules/bounds.rs"]
mod tests;
