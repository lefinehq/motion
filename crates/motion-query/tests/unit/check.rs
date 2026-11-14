//! Unit tests for `src/check.rs`.

use crate::check;
use crate::report::{Code, Report, Severity, Violation};
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::ops::RangeInclusive;

/// A typesetter with the built-in fonts.
fn typesetter() -> motion_text::Typesetter {
    motion_text::Typesetter::new(motion_text::FontSet::builtin())
}

/// Checks the scene in `json`.
fn check_json(json: &str) -> Report {
    let scene = motion_format::parse_document(json).unwrap().scene().clone();
    let graph = motion_scene::compile(&scene, &mut typesetter()).unwrap();
    check(&scene, &graph)
}

/// Checks a one-second scene with one layer.
fn one(layer: &str) -> Report {
    check_json(&format!(r#"{{"motion":1,"dur":"1s","layers":[{layer}]}}"#))
}

fn report(violations: Vec<Violation>, passed: usize) -> Report {
    Report { violations, passed }
}

fn offscreen(
    id: &str,
    frames: RangeInclusive<u32>,
    amount_px: f64,
    fix: Option<&str>,
) -> Violation {
    violation(Severity::Error, Code::Offscreen, id, frames, amount_px, fix)
}

fn unsafe_area(
    id: &str,
    frames: RangeInclusive<u32>,
    amount_px: f64,
    fix: Option<&str>,
) -> Violation {
    violation(Severity::Warning, Code::Unsafe, id, frames, amount_px, fix)
}

fn violation(
    severity: Severity,
    code: Code,
    id: &str,
    frames: RangeInclusive<u32>,
    amount_px: f64,
    fix: Option<&str>,
) -> Violation {
    Violation {
        severity,
        code,
        id: id.into(),
        first_frame: *frames.start(),
        last_frame: *frames.end(),
        amount_px,
        fix: fix.map(String::from),
    }
}

/// The `at` value of the first fix hint in `report`.
fn fixed_at(report: &Report) -> &str {
    let fix = report
        .violations
        .iter()
        .find_map(|v| v.fix.as_deref())
        .expect("a fix hint");
    let (_, at) = fix.split_once("at=").expect("an `at` edit");
    at.trim_matches('"')
}

fn has_hint(report: &Report) -> bool {
    report.violations.iter().any(|v| v.fix.is_some())
}

#[rstest]
#[case::clean_scene_reports_ok_count(
    include_str!("../../../../examples/launch.json"),
    report(vec![], 4)
)]
#[case::offscreen_reports_range_amount_and_fix(
    r#"{"motion":1,"dur":"5s","layers":[{"id":"card","rect":"300x400","at":"center 1000"}]}"#,
    report(vec![offscreen("card", 0..=149, 120.0, Some(r#"card at="center 826""#))], 0)
)]
#[case::unsafe_area_is_a_warning(
    r#"{"motion":1,"dur":"5s","layers":[{"id":"card","rect":"300x100","at":"center 1010"}]}"#,
    report(vec![unsafe_area("card", 0..=149, 34.0, Some(r#"card at="center 976""#))], 1)
)]
// 10.5 frames: frames 0..=10 are evaluated.
#[case::fractional_duration_evaluates_frames_up_to_the_last_partial_one(
    r#"{"motion":1,"dur":"10.5f","layers":[{"id":"a","rect":"100x100","at":"30 center"}]}"#,
    report(vec![offscreen("a", 0..=10, 20.0, Some(r#"a at="146 center""#))], 0)
)]
// Moving `at` only moves the end of a track that starts at x=1900, so a
// hint on x would never converge.
#[case::absolute_preset_start_gets_no_hint_on_that_axis(
    r#"{"motion":1,"dur":"1s","presets":{"swoop":{"x":1900}},"layers":[{"id":"a","rect":"300x100","at":"center center","in":"swoop 12f linear"}]}"#,
    report(vec![offscreen("a", 0..=1, 130.0, None)], 0)
)]
#[case::empty_scene_is_ok_zero(r#"{"motion":1,"dur":"1s","layers":[]}"#, report(vec![], 0))]
fn reports(#[case] json: &str, #[case] expected: Report) {
    assert_eq!(check_json(json), expected);
}

/// A one-second scene with the one layer in each case.
#[rstest]
#[case::entrance_overshoot_is_caught_only_while_visible(
    r#"{"id":"card","rect":"300x100","at":"center 1000","in":"fade-up 60px 12f linear @0f"}"#,
    report(vec![offscreen("card", 1..=5, 25.0, Some(r#"card at="center 921""#))], 0)
)]
#[case::left_edge_fix_adds_and_resolves_only_the_moving_axis(
    r#"{"id":"a","rect":"200x100","at":"50 center"}"#,
    report(vec![offscreen("a", 0..=29, 50.0, Some(r#"a at="196 center""#))], 0)
)]
// Right edge 130 px out, bottom edge 20 px out.
#[case::corner_overshoot_fixes_both_axes_in_one_hint(
    r#"{"id":"c","rect":"300x200","at":"1900 1000"}"#,
    report(vec![offscreen("c", 0..=29, 130.0, Some(r#"c at="1674 926""#))], 0)
)]
#[case::right_edge_fix_subtracts(
    r#"{"id":"r","rect":"200x100","at":"1900 center"}"#,
    report(vec![offscreen("r", 0..=29, 80.0, Some(r#"r at="1724 center""#))], 0)
)]
#[case::top_edge_fix_adds(
    r#"{"id":"t","rect":"200x100","at":"center 20"}"#,
    report(vec![offscreen("t", 0..=29, 30.0, Some(r#"t at="center 104""#))], 0)
)]
// 2000 wide on a 1920 canvas is bleed.
#[case::oversize_axis_is_not_measured(
    r#"{"id":"w","rect":"2000x100","at":"center center"}"#,
    report(vec![], 2)
)]
#[case::oversize_axis_is_not_measured_and_the_other_still_is(
    r#"{"id":"w","rect":"2000x100","at":"center 1070"}"#,
    report(vec![offscreen("w", 0..=29, 40.0, Some(r#"w at="center 976""#))], 0)
)]
#[case::full_bleed_background_has_no_violations(
    r#"{"id":"bg","rect":"1920x1080","at":"center center"}"#,
    report(vec![], 2)
)]
// 400x40 turned 90 degrees is 400 tall: from y=100 it reaches 100 px above
// the canvas, and the fix aims at the safe area, 54 px in.
#[case::a_rotated_box_leaving_the_canvas_is_caught(
    r#"{"id":"r","rect":"400x40","at":"150 100","anim":{"rotate":"90 -> 90 @0-10f"}}"#,
    report(vec![offscreen("r", 0..=29, 100.0, Some(r#"r at="150 254""#))], 0)
)]
// x is driven by anim, so only y moves: from 10 px out at the top into the safe area.
#[case::an_anim_driven_axis_gets_no_fix_shift(
    r#"{"id":"a","rect":"100x100","at":"500 40","anim":{"x":"-20 -> -20 @0-10f"}}"#,
    report(vec![offscreen("a", 0..=29, 70.0, Some(r#"a at="500 104""#))], 0)
)]
// x is anim-driven and off the left edge; y is fine.
#[case::no_hint_when_no_axis_can_move(
    r#"{"id":"a","rect":"100x100","at":"500 250","anim":{"x":"-20 -> -20 @0-10f"}}"#,
    report(vec![offscreen("a", 0..=29, 70.0, None)], 0)
)]
fn one_layer_reports(#[case] layer: &str, #[case] expected: Report) {
    assert_eq!(one(layer), expected);
}

#[rstest]
#[case("300x200", "1900 1000")]
#[case("300x400", "center 1000")]
#[case("200x100", "50 center")]
#[case("200x100", "center 20")]
#[case("300x100", "center 1010")]
#[case("1700x100", "1000 center")]
fn applying_a_fix_makes_the_rules_pass(#[case] rect: &str, #[case] at: &str) {
    let before = one(&format!(r#"{{"id":"n","rect":"{rect}","at":"{at}"}}"#));
    let moved = fixed_at(&before);
    let after = one(&format!(r#"{{"id":"n","rect":"{rect}","at":"{moved}"}}"#));
    assert_eq!(after, report(vec![], 2), "moved to {moved}");
}

/// An entrance sweeps a larger box than any one frame. After one edit there
/// is no error and no hint (a warning is fine when the sweep cannot fit).
#[rstest]
#[case("300x950", "center 560", "fade-up 60px 12f linear")]
#[case("1700x100", "center center", "fade-left 60px 12f linear")]
#[case("300x200", "1900 1150", "fade-right 60px 12f linear")]
#[case("300x100", "center 1000", "fade-up 60px 12f linear")]
fn hints_for_animated_layers_settle_in_one_edit(
    #[case] rect: &str,
    #[case] at: &str,
    #[case] enter: &str,
) {
    let layer = |at: &str| {
        one(&format!(
            r#"{{"id":"n","rect":"{rect}","at":"{at}","in":"{enter}"}}"#
        ))
    };
    let mut found = layer(at);
    if has_hint(&found) {
        found = layer(fixed_at(&found));
    }
    assert!(!has_hint(&found) && !found.has_errors(), "{found:?}");
}

#[test]
fn a_hint_also_clears_the_safe_area_on_the_other_axis() {
    // Off the canvas on the right, only outside the safe area at the bottom.
    let before = one(r#"{"id":"c","rect":"300x200","at":"1900 980"}"#);
    assert_eq!(
        before,
        report(
            vec![offscreen("c", 0..=29, 130.0, Some(r#"c at="1674 926""#))],
            0
        )
    );
    let after = one(&format!(
        r#"{{"id":"c","rect":"300x200","at":"{}"}}"#,
        fixed_at(&before)
    ));
    assert_eq!(after, report(vec![], 2));
}

#[test]
fn run_shows_the_worst_overshoot_not_the_first() {
    // At rest 20 px past the bottom; the entrance slides down towards it, so
    // the overshoot grows frame by frame.
    let found = one(
        r#"{"id":"card","rect":"300x100","at":"center 1050","in":"fade-down 60px 12f linear @0f"}"#,
    );
    let [run] = &found.violations[..] else {
        panic!("one run: {found:?}");
    };
    assert_eq!(
        (run.code, run.last_frame, run.amount_px, run.fix.as_deref()),
        (Code::Offscreen, 29, 20.0, Some(r#"card at="center 976""#))
    );
}

/// The one test that pins the report's text format (`cargo insta review`).
#[test]
fn report_with_several_violations_snapshot() {
    let report = check_json(concat!(
        r#"{"motion":1,"dur":"1s","layers":["#,
        r#"{"id":"a","rect":"300x400","at":"center 1000"},"#,
        r#"{"id":"b","rect":"300x100","at":"center 1010"},"#,
        r#"{"id":"c","rect":"200x100","at":"50 center"}"#,
        r#"]}"#
    ))
    .to_string();
    insta::assert_snapshot!(report);
}

/// The fix of the first violation of `report`.
fn first_fix(report: &Report) -> Option<&str> {
    report.violations.first().and_then(|v| v.fix.as_deref())
}

#[test]
fn a_fix_under_a_scaled_parent_moves_at_in_the_parents_units() {
    // The parent triples everything: the child spans 990..1050 on the canvas.
    let r = check_json(
        r#"{"motion":1,"size":"1000x1000","dur":"1s","layers":[
        {"id":"box","rect":"100x100","at":"0 0","anchor":[0,0],"scale":3},
        {"id":"kid","rect":"20x20","parent":"box","at":"340 50"}]}"#,
    );
    let kid = r.violations.iter().find(|v| v.id == "kid").unwrap();
    assert_eq!(kid.code, Code::Offscreen);
    // The safe area ends at 950: 100 canvas px back is 33.33 of the parent's units.
    let fix = kid.fix.as_deref().unwrap();
    assert!(fix.starts_with(r#"kid at="306.67 "#), "{fix}");
}

#[test]
fn a_fix_under_a_turned_parent_follows_its_axes() {
    // Turned a quarter: the child's own x runs down the canvas.
    let r = check_json(
        r#"{"motion":1,"size":"1000x1000","dur":"1s","layers":[
        {"id":"box","rect":"100x100","at":"500 500","anchor":[0,0],"rotate":90},
        {"id":"kid","rect":"20x20","parent":"box","at":"490 0"}]}"#,
    );
    // Too low on the canvas, so its own x must shrink.
    let fix = first_fix(&r).unwrap();
    assert!(fix.starts_with(r#"kid at="440 "#), "{fix}");
}

#[test]
fn no_fix_while_the_parent_moves() {
    let r = check_json(
        r#"{"motion":1,"size":"1000x1000","fps":10,"dur":"1s","layers":[
        {"id":"box","rect":"100x100","at":"0 0","anchor":[0,0],"anim":{"x":"0 -> 100 @0-10f"}},
        {"id":"kid","rect":"20x20","parent":"box","at":"2000 50"}]}"#,
    );
    let kid = r.violations.iter().find(|v| v.id == "kid").unwrap();
    assert_eq!(kid.fix, None);
}
