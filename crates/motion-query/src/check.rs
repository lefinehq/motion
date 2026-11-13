//! The `check` query: run every rule over every frame of a scene.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). Frames are sampled at whole numbers, each rule is
//! asked about each node, and consecutive failing frames become one
//! [`Violation`] so the report stays short.

use crate::report::{Report, Violation};
use crate::rules::{Overshoot, RULES, Rule};
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
            fix: fix(id, at, self.worst.shift, graph),
        }
    }
}

/// Builds `id at="x y"` from the rule's shift, or `None` if nothing moves.
///
/// An axis that does not move keeps its original coordinate (so `center`
/// stays `center`); a moving axis resolves `center` to pixels first.
fn fix(id: &str, at: Position, (dx, dy): (f64, f64), graph: &SceneGraph) -> Option<String> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    let moved = |c: Coord, delta: f64, canvas: f64| match (c, delta) {
        (c, d) if d == 0.0 => c,
        (Coord::Px(v), d) => Coord::Px(v + d),
        (Coord::Center, d) => Coord::Px(canvas / 2.0 + d),
    };
    let (x, y) = (moved(at.x, dx, graph.width), moved(at.y, dy, graph.height));
    Some(format!("{id} at=\"{x} {y}\""))
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
            "E offscreen card 0-149f +120px; fix: card at=\"center 826\"\nOK 0"
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
            "E offscreen card 1-5f +25px; fix: card at=\"center 921\"\nOK 0"
        );
    }
    #[test]
    fn left_edge_fix_adds_and_resolves_only_the_moving_axis() {
        assert_eq!(
            run(
                r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"200x100","at":"50 center"}]}"#
            ),
            "E offscreen a 0-29f +50px; fix: a at=\"196 center\"\nOK 0"
        );
    }
    /// A one-second scene with one layer.
    fn one(layer: &str) -> String {
        run(&format!(r#"{{"motion":1,"dur":"1s","layers":[{layer}]}}"#))
    }

    /// Takes the `at` out of the first fix hint in `report`.
    fn fixed_at(report: &str) -> String {
        let start = report.find("at=\"").expect("a fix hint") + 4;
        let rest = &report[start..];
        rest[..rest.find('"').unwrap()].to_string()
    }

    #[test]
    fn corner_overshoot_fixes_both_axes_in_one_hint() {
        // Right edge 130 px out, bottom edge 20 px out.
        assert_eq!(
            one(r#"{"id":"c","rect":"300x200","at":"1900 1000"}"#),
            "E offscreen c 0-29f +130px; fix: c at=\"1674 926\"\nOK 0"
        );
    }

    #[test]
    fn right_edge_fix_subtracts() {
        assert_eq!(
            one(r#"{"id":"r","rect":"200x100","at":"1900 center"}"#),
            "E offscreen r 0-29f +80px; fix: r at=\"1724 center\"\nOK 0"
        );
    }

    #[test]
    fn top_edge_fix_adds() {
        assert_eq!(
            one(r#"{"id":"t","rect":"200x100","at":"center 20"}"#),
            "E offscreen t 0-29f +30px; fix: t at=\"center 104\"\nOK 0"
        );
    }

    #[test]
    fn oversize_axis_is_not_measured_and_the_other_still_is() {
        // 2000 wide on a 1920 canvas is bleed; nothing to report when centred.
        assert_eq!(
            one(r#"{"id":"w","rect":"2000x100","at":"center center"}"#),
            "OK 2"
        );
        // The vertical overshoot is still found, and the fix leaves x alone.
        assert_eq!(
            one(r#"{"id":"w","rect":"2000x100","at":"center 1070"}"#),
            "E offscreen w 0-29f +40px; fix: w at=\"center 976\"\nOK 0"
        );
    }

    #[test]
    fn full_bleed_background_has_no_violations() {
        assert_eq!(
            one(r#"{"id":"bg","rect":"1920x1080","at":"center center"}"#),
            "OK 2"
        );
    }

    #[test]
    fn applying_a_fix_makes_the_rules_pass() {
        // Each layer is placed, checked, moved to the hinted position and checked again.
        for (rect, at) in [
            ("300x200", "1900 1000"),
            ("300x400", "center 1000"),
            ("200x100", "50 center"),
            ("200x100", "center 20"),
            ("300x100", "center 1010"),
            ("1700x100", "1000 center"),
        ] {
            let before = one(&format!(r#"{{"id":"n","rect":"{rect}","at":"{at}"}}"#));
            assert!(before.contains("fix:"), "{rect} at {at}: {before}");
            let moved = fixed_at(&before);
            let after = one(&format!(r#"{{"id":"n","rect":"{rect}","at":"{moved}"}}"#));
            assert_eq!(after, "OK 2", "{rect} at {at} -> {moved}: {after}");
        }
    }

    #[test]
    fn fractional_duration_evaluates_frames_up_to_the_last_partial_one() {
        // 10.5 frames: frames 0..=10 are evaluated.
        let json =
            r#"{"motion":1,"dur":"10.5f","layers":[{"id":"a","rect":"100x100","at":"30 center"}]}"#;
        assert_eq!(
            run(json),
            "E offscreen a 0-10f +20px; fix: a at=\"146 center\"\nOK 0"
        );
    }

    #[test]
    fn run_shows_the_worst_overshoot_not_the_first() {
        // Rest position is 20 px past the bottom; the entrance starts above and
        // slides down, so the overshoot grows frame by frame.
        let report = one(
            r#"{"id":"card","rect":"300x100","at":"center 1050","in":"fade-down 60px 12f linear @0f"}"#,
        );
        assert!(
            report.starts_with("E offscreen card ")
                && report.contains("-29f +20px; fix: card at=\"center 976\""),
            "{report}"
        );
    }

    #[test]
    fn empty_scene_is_ok_zero() {
        assert_eq!(run(r#"{"motion":1,"dur":"1s","layers":[]}"#), "OK 0");
    }
}
