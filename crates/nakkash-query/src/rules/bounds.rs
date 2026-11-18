//! Bounds rules: `offscreen` for the canvas, `unsafe` for the title-safe area inside it.

use super::{Overshoot, Rule, draws, overshoot, visible};
use crate::report::{Code, Severity};
use kurbo::Rect;
use nakkash_scene::{ContentState, SceneGraph, Visit};

/// The title-safe area keeps this fraction of the canvas free on each side.
const SAFE_INSET: f64 = 0.05;

/// A visible node must lie inside the canvas.
pub(super) const OFFSCREEN: Rule = Rule {
    code: Code::Offscreen,
    severity: Severity::Error,
    suppressed_by: None,
    evaluate: offscreen,
    // Aim at the safe area so one edit clears both rules.
    areas: |graph| (canvas(graph), safe_area(graph)),
};

/// A visible node should lie inside the title-safe area.
pub(super) const UNSAFE: Rule = Rule {
    code: Code::Unsafe,
    severity: Severity::Warning,
    // An off-screen node is also outside the safe area; say it once.
    suppressed_by: Some(Code::Offscreen),
    evaluate: outside_safe_area,
    areas: |graph| (safe_area(graph), safe_area(graph)),
};

/// The whole canvas.
fn canvas(graph: &SceneGraph) -> Rect {
    Rect::new(0.0, 0.0, graph.root.width, graph.root.height)
}

/// The title-safe area of the canvas.
fn safe_area(graph: &SceneGraph) -> Rect {
    let (dx, dy) = (
        graph.root.width * SAFE_INSET,
        graph.root.height * SAFE_INSET,
    );
    Rect::new(dx, dy, graph.root.width - dx, graph.root.height - dy)
}

fn offscreen(visit: &Visit<'_>, graph: &SceneGraph) -> Option<Overshoot> {
    if !(visible(visit) && draws(visit)) || shows_a_comp(visit) {
        return None;
    }
    overshoot(visit, canvas(graph))
}

fn outside_safe_area(visit: &Visit<'_>, graph: &SceneGraph) -> Option<Overshoot> {
    if !(visible(visit) && draws(visit)) || shows_a_comp(visit) {
        return None;
    }
    overshoot(visit, safe_area(graph))
}

/// Whether the node shows a composition. Its box is the composition's
/// canvas, which may be larger than what it draws there; what it draws is
/// measured at its own layers, at any depth, so the canvas is not, and a
/// composition placed, scaled or cropped whole is judged by its content.
fn shows_a_comp(visit: &Visit<'_>) -> bool {
    matches!(visit.node.content, ContentState::Comp(..))
}

#[cfg(test)]
#[path = "../../tests/unit/rules/bounds.rs"]
mod tests;
