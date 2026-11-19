//! Unit tests for `src/check.rs`.

use crate::check;
use crate::report::{Code, Measure, Report, Severity, Unit, Violation};
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::ops::RangeInclusive;

/// A typesetter with the built-in fonts.
fn typesetter() -> nakkash_text::Typesetter {
    nakkash_text::Typesetter::new(nakkash_text::FontSet::builtin())
}

/// Checks the scene in `json`.
fn check_json(json: &str) -> Report {
    let scene = nakkash_format::parse_document(json)
        .unwrap()
        .scene()
        .clone();
    let graph = nakkash_scene::compile(&scene, &mut typesetter()).unwrap();
    check(&scene, &graph)
}

/// Checks a one-second scene with one layer.
fn one(layer: &str) -> Report {
    check_json(&format!(r#"{{"nakkash":1,"dur":"1s","layers":[{layer}]}}"#))
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

/// A move at one speed over `frames`: a peak no higher than its mean.
fn linear(id: &str, frames: RangeInclusive<u32>) -> Violation {
    Violation {
        measure: Measure::Below {
            value: 1.0,
            limit: 1.4,
            unit: Unit::Number,
        },
        ..violation(Severity::Warning, Code::Easing, id, frames, 0.0, None)
    }
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
        address: id.parse().unwrap(),
        first_frame: *frames.start(),
        last_frame: *frames.end(),
        measure: Measure::Overshoot(amount_px),
        fix: fix.map(String::from),
        with: None,
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
    // Bounds on both layers, a judder and an easing pass on each entrance,
    // a text hold, no crowd.
    report(vec![], 10)
)]
#[case::offscreen_reports_range_amount_and_fix(
    r#"{"nakkash":1,"dur":"5s","layers":[{"id":"card","rect":"300x400","at":"center 1000"}]}"#,
    report(vec![offscreen("card", 0..=149, 120.0, Some(r#"card at="center 826""#))], 0)
)]
#[case::unsafe_area_is_a_warning(
    r#"{"nakkash":1,"dur":"5s","layers":[{"id":"card","rect":"300x100","at":"center 1010"}]}"#,
    report(vec![unsafe_area("card", 0..=149, 34.0, Some(r#"card at="center 976""#))], 1)
)]
// 10.5 frames: frames 0..=10 are evaluated.
#[case::fractional_duration_evaluates_frames_up_to_the_last_partial_one(
    r#"{"nakkash":1,"dur":"10.5f","layers":[{"id":"a","rect":"100x100","at":"30 center"}]}"#,
    report(vec![offscreen("a", 0..=10, 20.0, Some(r#"a at="146 center""#))], 0)
)]
// Moving `at` only moves the end of a track that starts at x=1900, so a
// hint on x would never converge.
#[case::absolute_preset_start_gets_no_hint_on_that_axis(
    r#"{"nakkash":1,"dur":"1s","presets":{"swoop":{"x":1900}},"layers":[{"id":"a","rect":"300x100","at":"center center","in":"swoop 12f linear"}]}"#,
    // It swoops in from x 1900 in 12 frames at one speed: about 79 px a
    // frame, through the right edge for two frames, which is brief.
    report(
        vec![
            Violation {
                measure: Measure::Above {
                    value: 79.0,
                    limit: 11.0,
                    unit: Unit::PxPerFrame,
                },
                ..violation(
                    Severity::Warning,
                    Code::Judder,
                    "a",
                    1..=12,
                    0.0,
                    Some("a mblur=true"),
                )
            },
            linear("a", 0..=12),
        ],
        // The crowd and both layout rules pass.
        3,
    )
)]
#[case::empty_scene_is_ok_zero(r#"{"nakkash":1,"dur":"1s","layers":[]}"#, report(vec![], 0))]
fn reports(#[case] json: &str, #[case] expected: Report) {
    assert_eq!(check_json(json), expected);
}

/// A one-second scene with the one layer in each case.
#[rstest]
#[case::an_entrance_rising_through_the_edge_is_not_offscreen(
    r#"{"id":"card","rect":"300x100","at":"center 1000","in":"fade-up 60px 12f linear @0f"}"#,
    // It crosses the bottom edge for frames 1 to 5 of its rise, then rests
    // inside, though past the safe area: a brief pass in motion, then a rest.
    // The 60 px rise takes 5 px a frame, at one speed: a judder pass, an
    // easing finding and a crowd pass.
    report(
        vec![
            Violation {
                measure: Measure::Overshoot(79.0),
                ..violation(
                    Severity::Warning,
                    Code::Unsafe,
                    "card",
                    1..=29,
                    0.0,
                    Some(r#"card at="center 921""#),
                )
            },
            linear("card", 1..=12),
        ],
        3,
    )
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
    let runs: Vec<&Violation> = found
        .violations
        .iter()
        .filter(|v| v.code == Code::Offscreen)
        .collect();
    let [run] = runs[..] else {
        panic!("one run: {found:?}");
    };
    assert_eq!(
        (run.code, run.last_frame, run.measure, run.fix.as_deref()),
        (
            Code::Offscreen,
            29,
            Measure::Overshoot(20.0),
            Some(r#"card at="center 976""#)
        )
    );
}

/// The one test that pins the report's text format (`cargo insta review`).
#[test]
fn report_with_several_violations_snapshot() {
    let report = check_json(concat!(
        r#"{"nakkash":1,"dur":"1s","layers":["#,
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
        r#"{"nakkash":1,"size":"1000x1000","dur":"1s","layers":[
        {"id":"box","rect":"100x100","at":"0 0","anchor":[0,0],"scale":3},
        {"id":"kid","rect":"20x20","parent":"box","at":"340 50"}]}"#,
    );
    let kid = r
        .violations
        .iter()
        .find(|v| v.address.to_string() == "kid")
        .unwrap();
    assert_eq!(kid.code, Code::Offscreen);
    // The safe area ends at 950: 100 canvas px back is 33.33 of the parent's units.
    let fix = kid.fix.as_deref().unwrap();
    assert!(fix.starts_with(r#"kid at="306.67 "#), "{fix}");
}

#[test]
fn a_fix_under_a_turned_parent_follows_its_axes() {
    // Turned a quarter: the child's own x runs down the canvas.
    let r = check_json(
        r#"{"nakkash":1,"size":"1000x1000","dur":"1s","layers":[
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
        r#"{"nakkash":1,"size":"1000x1000","fps":10,"dur":"1s","layers":[
        {"id":"box","rect":"100x100","at":"0 0","anchor":[0,0],"anim":{"x":"0 -> 100 @0-10f"}},
        {"id":"kid","rect":"20x20","parent":"box","at":"2000 50"}]}"#,
    );
    let kid = r
        .violations
        .iter()
        .find(|v| v.address.to_string() == "kid")
        .unwrap();
    assert_eq!(kid.fix, None);
}

/// A 1000x1000 scene whose `card` layer, centred at `card_x`, shows the
/// 400x200 composition `card`, which holds a 10x10 `title` centred at
/// `title_x` in the composition's canvas.
fn nested(card_x: u32, title_x: u32) -> Report {
    nested_with(card_x, title_x, "")
}

/// [`nested`], with `kv` on the `card` layer.
fn nested_with(card_x: u32, title_x: u32, kv: &str) -> Report {
    check_json(&format!(
        r#"{{"nakkash":1,"size":"1000x1000","dur":"1s",
        "comps":{{"card":{{"size":"400x200","dur":"1s",
          "layers":[{{"id":"title","rect":"10x10","at":"{title_x} 100"}}]}}}},
        "layers":[{{"id":"card","comp":"card","position":[{card_x},500]{kv}}}]}}"#
    ))
}

#[test]
fn a_layer_inside_a_comp_is_checked_in_scene_space_by_its_address() {
    // The card runs 650..1050, the title 1040..1050: both 50 px past the
    // edge. The card shows a composition, so what it draws is the title,
    // which is found at its own address; the card's canvas is not measured.
    let r = nested(850, 395);
    let found: Vec<(String, Code, Measure, bool)> = r
        .violations
        .iter()
        .map(|v| (v.address.to_string(), v.code, v.measure, v.fix.is_some()))
        .collect();
    assert_eq!(
        found,
        [(
            "card/title".into(),
            Code::Offscreen,
            Measure::Overshoot(50.0),
            true
        )]
    );
}

#[test]
fn what_a_comp_clips_away_is_never_offscreen() {
    // Inside the canvas, the title sits past the composition's own edge.
    assert_eq!(nested(500, 395).violations, []);
    assert_eq!(nested(500, 420).violations, []);
    assert_eq!(nested(500, 420).passed, 4);
}

/// The layout findings of `report`: each layer's address, rule, frames and amount.
fn layout(report: &Report) -> Vec<(String, Code, RangeInclusive<u32>, Measure)> {
    report
        .violations
        .iter()
        .filter(|v| matches!(v.code, Code::Offscreen | Code::Unsafe))
        .map(|v| {
            let frames = v.first_frame..=v.last_frame;
            (v.address.to_string(), v.code, frames, v.measure)
        })
        .collect()
}

/// A one-second 1000x1000 scene with `layers`.
fn square(layers: &str) -> Report {
    check_json(&format!(
        r#"{{"nakkash":1,"size":"1000x1000","dur":"1s","layers":[{layers}]}}"#
    ))
}

#[rstest]
// The title runs 910..1010 and the slot 850..950: it shows inside the slot.
#[case::shown_inside_its_matte("slot", vec![])]
#[case::inverted_shows_it_outside(
    "slot alpha-inverted",
    vec![("title".into(), Code::Offscreen, 0..=29, Measure::Overshoot(10.0))]
)]
fn what_a_matte_hides_is_never_offscreen(
    #[case] matte: &str,
    #[case] found: Vec<(String, Code, RangeInclusive<u32>, Measure)>,
) {
    let r = square(&format!(
        r#"{{"id":"slot","rect":"400x100","position":[500,900],"hidden":true}},
        {{"id":"title","rect":"400x100","position":[500,960],"matte":"{matte}"}}"#
    ));
    assert_eq!(layout(&r), found);
}

#[test]
fn a_text_rising_inside_its_matte_is_never_offscreen() {
    // It rises from wholly below the slot, so first nothing of it shows.
    let r = square(
        r#"{"id":"slot","rect":"400x100","position":[500,900],"hidden":true},
        {"id":"title","rect":"400x100","position":[500,900],"matte":"slot",
         "anim":{"y":"1010 -> 900 @0-10f"}}"#,
    );
    assert_eq!(layout(&r), []);
}

#[rstest]
#[case::pushed_in_from_below(r#""anim":{"y":"1500 -> 500 @5-15f"}"#)]
#[case::pushed_out_at_the_top(r#""anim":{"y":"500 -> -500 @15-25f"}"#)]
#[case::pushed_through(r#""anim":{"y":"1500 -> -500 @0-29f"}"#)]
fn a_layer_moving_in_from_off_the_canvas_or_out_to_it_is_not_offscreen(#[case] anim: &str) {
    let r = square(&format!(r#"{{"id":"card","rect":"400x100",{anim}}}"#));
    assert_eq!(layout(&r), []);
}

#[test]
fn a_layer_that_comes_in_and_stops_across_the_edge_is_offscreen_once_it_rests() {
    // It comes to rest at 930..1030 at frame 10, then stays there.
    let r = square(r#"{"id":"card","rect":"400x100","anim":{"y":"1500 -> 980 @0-10f"}}"#);
    assert_eq!(
        layout(&r),
        [(
            "card".into(),
            Code::Offscreen,
            11..=29,
            Measure::Overshoot(30.0)
        )]
    );
}

#[rstest]
// A copy zooms through the right edge and is gone after a third of a
// second, as a glitch's copies do; at half the size it is judged alike.
#[case::a_zoom_through_the_edge(
    r#""rect":"400x100","position":[700,500],"span":"0f-10f","anim":{"scale":"1 -> 3 @0-10f"}"#
)]
#[case::the_same_zoom_at_half_the_size(
    r#""rect":"200x50","position":[850,500],"span":"0f-10f","anim":{"scale":"1 -> 3 @0-10f"}"#
)]
// It swings past the edge for a few frames and settles inside.
#[case::an_overshoot_past_the_edge(r#""rect":"400x100","anim":{"x":"500 -> 850 -> 700 @0-20f"}"#)]
fn a_layer_moving_briefly_across_the_edge_is_not_offscreen(#[case] keys: &str) {
    let r = square(&format!(r#"{{"id":"card",{keys}}}"#));
    assert_eq!(layout(&r), []);
}

#[test]
fn a_layer_moving_across_the_edge_for_longer_is_offscreen() {
    // It drifts out past the right edge for the whole second.
    let r = square(r#"{"id":"card","rect":"400x100","anim":{"x":"800 -> 1000 @0-29f"}}"#);
    assert_eq!(
        layout(&r),
        [(
            "card".into(),
            Code::Offscreen,
            1..=29,
            Measure::Overshoot(200.0)
        )]
    );
}

#[test]
fn a_pivot_draws_nothing_so_it_may_wait_anywhere() {
    // It waits far below, carrying the card up into view at the end.
    let r = square(
        r#"{"id":"push","pivot":true,"position":[500,500],"anim":{"y":"2500 -> 500 @20-29f"}},
        {"id":"card","rect":"400x100","parent":"push"}"#,
    );
    assert_eq!(layout(&r), []);
}

#[test]
fn a_layer_that_never_comes_onto_the_canvas_is_offscreen() {
    let r = square(r#"{"id":"card","rect":"400x100","position":[500,1500]}"#);
    assert_eq!(
        layout(&r),
        [(
            "card".into(),
            Code::Offscreen,
            0..=29,
            Measure::Overshoot(550.0)
        )]
    );
}

#[test]
fn the_fix_for_an_inner_layer_is_an_edit_of_its_instance() {
    let r = nested(850, 395);
    let title = r
        .violations
        .iter()
        .find(|v| v.address.to_string() == "card/title")
        .unwrap();
    let fix = title.fix.as_deref().unwrap();
    // The title must come 100 px in, to the safe area's edge at 950.
    assert_eq!(fix, r#"card/title at="295 100""#);
    // Written as the edit writes it, the override clears the title.
    let after = nested_with(850, 395, r#","overrides":{"title":{"at":"295 100"}}"#);
    assert!(
        after
            .violations
            .iter()
            .all(|v| v.address.to_string() != "card/title"),
        "{after}"
    );
}

/// The craft findings of `report`: everything but the layout rules.
fn craft(report: &Report) -> Vec<&Violation> {
    report
        .violations
        .iter()
        .filter(|v| !matches!(v.code, Code::Offscreen | Code::Unsafe))
        .collect()
}

#[test]
fn a_fast_move_judders_until_motion_blur_is_on() {
    // 1700 px in 10 frames: 170 px a frame, past the 11 px limit.
    let fast = r#""anim":{"x":"200 -> 1700 @0-10f"}"#;
    let found = one(&format!(r#"{{"id":"card","rect":"100x100",{fast}}}"#));
    let measure = Measure::Above {
        value: 150.0,
        limit: 11.0,
        unit: Unit::PxPerFrame,
    };
    let judders: Vec<&Violation> = craft(&found)
        .into_iter()
        .filter(|v| v.code == Code::Judder)
        .collect();
    assert_eq!(
        judders,
        [&violation(
            Severity::Warning,
            Code::Judder,
            "card",
            1..=10,
            0.0,
            Some("card mblur=true")
        )]
        .map(|v| Violation {
            measure,
            ..v.clone()
        })
        .iter()
        .collect::<Vec<_>>()
    );
    let blurred = one(&format!(
        r#"{{"id":"card","rect":"100x100","mblur":true,{fast}}}"#
    ));
    assert!(craft(&blurred).iter().all(|v| v.code != Code::Judder));
}

#[test]
fn text_shown_too_briefly_is_found_with_its_reading_time() {
    // 11 characters need 0.55 s; shown for 10 frames, a third of a second.
    let found = one(r#"{"id":"title","text":"Launch week","span":"0f-10f"}"#);
    let [hold] = craft(&found)[..] else {
        panic!("one finding: {found}");
    };
    assert_eq!(
        (hold.code, hold.first_frame, hold.last_frame, hold.measure),
        (
            Code::TextHold,
            0,
            9,
            Measure::Below {
                value: 0.3,
                limit: 0.6,
                unit: Unit::Seconds
            }
        )
    );
    let long_enough = one(r#"{"id":"title","text":"Launch week"}"#);
    assert_eq!(craft(&long_enough), Vec::<&Violation>::new());
}

#[test]
fn a_move_at_one_speed_is_found_and_an_eased_one_passes() {
    let linear = one(r#"{"id":"card","rect":"100x100","anim":{"x":"300 -> 900 @0-20f"}}"#);
    let found: Vec<(Code, Measure)> = craft(&linear).iter().map(|v| (v.code, v.measure)).collect();
    assert!(
        found.contains(&(
            Code::Easing,
            Measure::Below {
                value: 1.0,
                limit: 1.4,
                unit: Unit::Number
            }
        )),
        "{linear}"
    );
    let eased = one(r#"{"id":"card","rect":"100x100","anim":{"x":"300 -> 900 @0-20f out-cubic"}}"#);
    assert!(
        craft(&eased).iter().all(|v| v.code != Code::Easing),
        "{eased}"
    );
}

#[test]
fn five_layers_moving_at_once_crowd_the_scene() {
    let layer = |i: usize| {
        format!(
            r#"{{"id":"l{i}","rect":"50x50","at":"200 {}","anim":{{"x":"200 -> 400 @0-20f out-cubic"}}}}"#,
            150 + i * 150
        )
    };
    let five = (0..5).map(layer).collect::<Vec<_>>().join(",");
    let found = check_json(&format!(r#"{{"nakkash":1,"dur":"1s","layers":[{five}]}}"#));
    let crowds: Vec<&Violation> = craft(&found)
        .into_iter()
        .filter(|v| v.code == Code::Crowd)
        .collect();
    let [crowd] = crowds[..] else {
        panic!("one crowd: {found}");
    };
    assert_eq!(
        (crowd.address.to_string(), crowd.measure),
        (
            "scene".into(),
            Measure::Above {
                value: 5.0,
                limit: 4.0,
                unit: Unit::Number
            }
        )
    );
    let four = (0..4).map(layer).collect::<Vec<_>>().join(",");
    let calm = check_json(&format!(r#"{{"nakkash":1,"dur":"1s","layers":[{four}]}}"#));
    assert!(craft(&calm).iter().all(|v| v.code != Code::Crowd), "{calm}");
}

#[test]
fn the_peak_of_an_ease_is_too_brief_to_judder() {
    // Out of a sharp ease, the card steps 21, 16 and 13 px, then slows
    // under 11: 3 frames, under an eighth of a second.
    let brief = one(r#"{"id":"card","rect":"100x100","anim":{"x":"300 -> 400 @0-30f out-expo"}}"#);
    assert!(
        craft(&brief).iter().all(|v| v.code != Code::Judder),
        "{brief}"
    );
}

#[test]
fn text_read_long_enough_once_holds_whatever_its_brief_stretches() {
    // Read for 2 frames, then too fast to read for 10, then for 19.
    let r = one(
        r#"{"id":"title","text":"Launch week","at":"center 500","anim":{"y":[["0f",500],["1f",500],["6f",1500],["11f",500]]}}"#,
    );
    assert!(r.violations.iter().all(|v| v.code != Code::TextHold), "{r}");
}

/// Two lines of 100 px type, `title` over `sub`, with `sub` at `sub_y` and
/// `kv` on it.
fn two_lines(sub_y: u32, kv: &str) -> Report {
    check_json(&format!(
        r#"{{"nakkash":1,"size":"1000x1000","dur":"1s","layers":[
          {{"id":"sub","text":"8 haftada.","font":"Inter Variable 100 900","at":"center {sub_y}"{kv}}},
          {{"id":"title","text":"Sıfırdan 5K'ya,","font":"Inter Variable 100 900","at":"center 400"}}]}}"#
    ))
}

/// The `overlap` findings of `report`, as printed.
fn overlaps(report: &Report) -> Vec<String> {
    report
        .violations
        .iter()
        .filter(|v| v.code == Code::Overlap)
        .map(|v| format!("{} over {}", v.address, v.with.as_ref().unwrap()))
        .collect()
}

#[rstest]
#[case::apart(600, "", &[])]
#[case::run_into_each_other(430, "", &["title over sub"])]
// Faint text is not read, so it runs into nothing.
#[case::one_faint(430, r#","opacity":0.5"#, &[])]
#[case::hidden(430, r#","hidden":true"#, &[])]
fn text_running_into_text_is_found(#[case] sub_y: u32, #[case] kv: &str, #[case] found: &[&str]) {
    assert_eq!(overlaps(&two_lines(sub_y, kv)), found);
}

#[test]
fn an_overlap_prints_both_layers_and_how_deep() {
    let r = two_lines(430, "");
    let line = r
        .violations
        .iter()
        .find(|v| v.code == Code::Overlap)
        .unwrap()
        .to_string();
    assert!(line.starts_with("W overlap title 0-29f +"), "{line}");
    assert!(line.ends_with("px over sub"), "{line}");
}

/// A one-second scene at 30 fps whose `ui` rect is pinned by its corner
/// pivots; `br` animates the bottom-right pivot.
fn pinned(br: &str) -> Report {
    check_json(&format!(
        r#"{{"nakkash":1,"dur":"1s","fps":30,"layers":[
        {{"id":"screen-tl","pivot":true,"position":[700,300]}},
        {{"id":"screen-tr","pivot":true,"position":[1200,320]}},
        {{"id":"screen-br","pivot":true,"position":[1180,760]{br}}},
        {{"id":"screen-bl","pivot":true,"position":[720,740]}},
        {{"id":"ui","rect":"300x600","at":"center center","fx":[{{"corner-pin":[0,0],"pivots":"screen"}}]}}]}}"#
    ))
}

#[test]
fn a_corner_pin_onto_a_quad_passes() {
    let report = pinned("");
    assert!(
        report.violations.iter().all(|v| v.code != Code::Pin),
        "{report}"
    );
}

#[test]
fn a_pivot_that_crosses_its_neighbours_is_a_pin_error_for_the_frames_it_does() {
    // The bottom-right pivot swings past the bottom-left one from frame 10
    // to 20, crossing the quad's sides, and back.
    let report = pinned(
        r#","anim":{"x":[{"t":"10f","v":1180},{"t":"11f","v":400},{"t":"20f","v":400},{"t":"21f","v":1180}]}"#,
    );
    let pins: Vec<String> = report
        .violations
        .iter()
        .filter(|v| v.code == Code::Pin)
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        pins,
        ["E pin ui 11-20f crossed: top-right–bottom-right meets bottom-left–top-left"]
    );
    assert!(report.has_errors());
}

#[test]
fn a_layer_without_a_corner_pin_runs_no_pin_rule() {
    let without = one(r#"{"id":"a","rect":"100x100","at":"center center"}"#);
    let with =
        one(r#"{"id":"a","rect":"100x100","at":"center center","fx":[{"corner-pin":[0,0]}]}"#);
    assert_eq!(with.passed, without.passed + 1);
}

#[test]
fn a_text_wider_than_the_canvas_is_offscreen_though_a_shape_that_wide_is_bleed() {
    let report = one(
        r#"{"id":"t","text":"WIDER THAN THE FRAME","font":"Inter Variable 300 800","at":"center center"}"#,
    );
    let off: Vec<&Violation> = report
        .violations
        .iter()
        .filter(|v| v.code == Code::Offscreen)
        .collect();
    assert_eq!(off.len(), 1, "{report}");
    assert_eq!(off[0].address.to_string(), "t");
}
