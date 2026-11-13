//! The `check` query: run every rule over every frame of a scene.
//!
//! Part of step 4 (Query). Frames are sampled at whole numbers, each rule is
//! asked about each node, and consecutive failing frames become one
//! [`Violation`] so the report stays short.

use crate::report::{Report, Violation};
use crate::rules::{Edge, Overshoot, RULES, Rule};
use motion_eval::{FrameState, evaluate};
use motion_format::Scene;
use motion_format::shorthand::{Coord, Position};
use motion_scene::SceneGraph;

/// Runs the standard rules on `scene` over every frame of `graph`.
///
/// `graph` must be the compiled form of `scene`: nodes and layers are matched
/// by position.
pub fn check(scene: &Scene, graph: &SceneGraph) -> Report {
    // Whole frames 0..ceil(duration). Evaluate each once and share it.
    let frame_count = graph.duration_frames.ceil().max(0.0) as u32;
    let frames: Vec<FrameState> = (0..frame_count)
        .map(|f| evaluate(graph, f as f64))
        .collect();

    let mut violations = Vec::new();
    let mut passed = 0;
    for (index, layer) in scene.layers.iter().enumerate() {
        let mut failed_codes: Vec<&str> = Vec::new();
        for rule in RULES {
            if rule
                .suppressed_by
                .is_some_and(|c| failed_codes.contains(&c))
            {
                continue;
            }
            let found = runs(rule, index, &frames, graph);
            if found.is_empty() {
                passed += 1;
                continue;
            }
            failed_codes.push(rule.code);
            for run in found {
                violations.push(run.into_violation(rule, &layer.id, layer.at, graph));
            }
        }
    }
    Report { violations, passed }
}

/// A stretch of consecutive failing frames for one (rule, node).
struct Run {
    first: u32,
    last: u32,
    /// The worst overshoot in the stretch.
    worst: Overshoot,
}

/// Finds the failing stretches of `rule` on node `index`.
fn runs(rule: &Rule, index: usize, frames: &[FrameState], graph: &SceneGraph) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for (frame, state) in frames.iter().enumerate() {
        let frame = frame as u32;
        let Some(o) = (rule.evaluate)(&state.nodes[index], graph) else {
            continue;
        };
        match out.last_mut() {
            Some(run) if run.last + 1 == frame => {
                run.last = frame;
                if o.amount > run.worst.amount {
                    run.worst = o;
                }
            }
            _ => out.push(Run {
                first: frame,
                last: frame,
                worst: o,
            }),
        }
    }
    out
}

impl Run {
    fn into_violation(self, rule: &Rule, id: &str, at: Position, graph: &SceneGraph) -> Violation {
        // Round up to whole pixels, ignoring float noise: 60 * (11 / 12) is
        // 55.000000000000007 and must read +55, not +56.
        let amount = (self.worst.amount - crate::rules::EPSILON).ceil();
        Violation {
            severity: rule.severity,
            code: rule.code,
            id: id.to_string(),
            first_frame: self.first,
            last_frame: self.last,
            amount_px: amount,
            fix: Some(fix(id, at, self.worst.edge, amount, graph)),
        }
    }
}

/// Builds `id at="x y"`, moving the layer back inside along the axis of `edge`.
fn fix(id: &str, at: Position, edge: Edge, amount: f64, graph: &SceneGraph) -> String {
    // Resolve `center` to a number only on the axis that changes.
    let px = |c: Coord, canvas: f64| match c {
        Coord::Px(v) => v,
        Coord::Center => canvas / 2.0,
    };
    // Right and bottom overshoot: move back (subtract). Left and top: add.
    let (x, y) = match edge {
        Edge::Left => (Coord::Px(px(at.x, graph.width) + amount), at.y),
        Edge::Right => (Coord::Px(px(at.x, graph.width) - amount), at.y),
        Edge::Top => (at.x, Coord::Px(px(at.y, graph.height) + amount)),
        Edge::Bottom => (at.x, Coord::Px(px(at.y, graph.height) - amount)),
    };
    format!("{id} at=\"{x} {y}\"")
}

#[cfg(test)]
mod tests {
    use crate::check;
    fn run(json: &str) -> String {
        let s = motion_format::parse_document(json).unwrap().scene;
        let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
        check(&s, &g).to_string()
    }
    #[test]
    fn clean_scene_reports_ok_count() {
        assert_eq!(run(include_str!("../../../examples/launch.json")), "OK 4");
    }
    #[test]
    fn offscreen_reports_range_amount_and_fix() {
        assert_eq!(
            run(
                r#"{"motion":1,"dur":"5s","layers":[{"id":"card","rect":"300x400","at":"center 1000"}]}"#
            ),
            "E offscreen card 0-149f +120px; fix: card at=\"center 880\"\nOK 0"
        );
    }
    #[test]
    fn unsafe_area_is_a_warning() {
        assert_eq!(
            run(
                r#"{"motion":1,"dur":"5s","layers":[{"id":"card","rect":"300x100","at":"center 1010"}]}"#
            ),
            "W unsafe card 0-149f +34px; fix: card at=\"center 976\"\nOK 1"
        );
    }
    #[test]
    fn entrance_overshoot_is_caught_only_while_visible() {
        assert_eq!(
            run(
                r#"{"motion":1,"dur":"1s","layers":[{"id":"card","rect":"300x100","at":"center 1000","in":"fade-up 60px 12f linear @0f"}]}"#
            ),
            "E offscreen card 1-5f +25px; fix: card at=\"center 975\"\nOK 0"
        );
    }
    #[test]
    fn left_edge_fix_adds_and_resolves_only_the_moving_axis() {
        assert_eq!(
            run(
                r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"200x100","at":"50 center"}]}"#
            ),
            "E offscreen a 0-29f +50px; fix: a at=\"100 center\"\nOK 0"
        );
    }
    #[test]
    fn empty_scene_is_ok_zero() {
        assert_eq!(run(r#"{"motion":1,"dur":"1s","layers":[]}"#), "OK 0");
    }
}
