//! Unit tests for `src/check.rs`.

use crate::check;
fn run(json: &str) -> String {
    let s = motion_format::parse_document(json).unwrap().scene;
    let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
    check(&s, &g).to_string()
}
#[test]
fn clean_scene_reports_ok_count() {
    assert_eq!(
        run(include_str!("../../../../examples/launch.json")),
        "OK 4"
    );
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
        run(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"200x100","at":"50 center"}]}"#),
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
fn hints_for_animated_layers_settle_in_one_edit() {
    // Each entrance sweeps a larger box than any single frame. The hint is
    // applied once; after that there must be no error and no further hint
    // (a warning without a hint is fine: the sweep cannot fit the safe area).
    for (rect, at, enter) in [
        ("300x950", "center 560", "fade-up 60px 12f linear"),
        ("1700x100", "center center", "fade-left 60px 12f linear"),
        ("300x200", "1900 1150", "fade-right 60px 12f linear"),
        ("300x100", "center 1000", "fade-up 60px 12f linear"),
    ] {
        let layer = |at: &str| {
            one(&format!(
                r#"{{"id":"n","rect":"{rect}","at":"{at}","in":"{enter}"}}"#
            ))
        };
        let mut report = layer(at);
        if report.contains("fix:") {
            report = layer(&fixed_at(&report));
        }
        assert!(
            !report.contains("fix:") && !report.contains("E "),
            "{rect} at {at} {enter}: {report}"
        );
    }
}

#[test]
fn a_hint_also_clears_the_safe_area_on_the_other_axis() {
    // Off the canvas on the right, only outside the safe area at the bottom:
    // the offscreen hint fixes both, so no unsafe warning follows.
    let before = one(r#"{"id":"c","rect":"300x200","at":"1900 980"}"#);
    assert_eq!(
        before,
        "E offscreen c 0-29f +130px; fix: c at=\"1674 926\"\nOK 0"
    );
    let after = one(&format!(
        r#"{{"id":"c","rect":"300x200","at":"{}"}}"#,
        fixed_at(&before)
    ));
    assert_eq!(after, "OK 2");
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

/// Snapshot of a whole report with several violations, so a change to the
/// text format shows up as one reviewable diff (`cargo insta review`).
#[test]
fn report_with_several_violations_snapshot() {
    let report = run(concat!(
        r#"{"motion":1,"dur":"1s","layers":["#,
        r#"{"id":"a","rect":"300x400","at":"center 1000"},"#,
        r#"{"id":"b","rect":"300x100","at":"center 1010"},"#,
        r#"{"id":"c","rect":"200x100","at":"50 center"}"#,
        r#"]}"#
    ));
    insta::assert_snapshot!(report);
}
