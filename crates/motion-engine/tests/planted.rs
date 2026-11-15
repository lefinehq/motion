//! Planted defects ([D59] calibration): one scene per check rule, each with
//! one defect a motion designer would fix, trips its rule and no other.
//!
//! [D59]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d59-craft-lint-measures-motion-from-the-scene-and-reports-numbers

use std::collections::BTreeSet;
use std::str::FromStr;

use motion_engine::Session;
use rstest::rstest;

/// The rules `layers`, in a 2 s 1080p scene on black, trip: the scene's
/// rules and the pixel gates, as `motion check` runs them.
fn tripped(layers: &str) -> BTreeSet<String> {
    let json = format!(
        r##"{{"motion":1,"size":"1080p","fps":30,"dur":"2s","bg":"#000000","layers":[{layers}]}}"##
    );
    let session = Session::from_str(&json).expect("scene");
    let mut report = session.check().expect("check");
    report.join(session.gates().expect("gates"));
    report
        .violations
        .iter()
        .map(|v| v.code.to_string())
        .collect()
}

/// `n` cards, one above another, sliding right together with an ease, no
/// faster than 10 px a frame.
fn sliding(n: usize) -> String {
    (0..n)
        .map(|i| {
            format!(
                r##"{{"id":"card{i}","rect":"100x100","fill":"#FFFFFF","at":"400 {}","anim":{{"x":"400 -> 500 @0-30f out-cubic"}}}}"##,
                200 + i * 150
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// A full-frame white rect, on and off every 3 frames for a second.
fn blinking() -> String {
    let keys: Vec<String> = (0..10)
        .map(|k| format!(r#"["{}f",{},"hold"]"#, k * 3, k % 2))
        .collect();
    format!(
        r##"{{"id":"strobe","rect":"1920x1080","fill":"#FFFFFF","at":"center center","anim":{{"opacity":[{}]}}}}"##,
        keys.join(",")
    )
}

#[rstest]
#[case::offscreen(
    r##"{"id":"card","rect":"300x100","fill":"#FFFFFF","at":"center 1100"}"##,
    "offscreen"
)]
#[case::unsafe_area(
    r##"{"id":"card","rect":"300x100","fill":"#FFFFFF","at":"center 1010"}"##,
    "unsafe"
)]
#[case::judder(
    r##"{"id":"card","rect":"100x100","fill":"#FFFFFF","at":"200 center","anim":{"x":"200 -> 1700 @0-10f out-cubic"}}"##,
    "judder"
)]
#[case::easing(
    r##"{"id":"card","rect":"100x100","fill":"#FFFFFF","at":"300 center","anim":{"x":"300 -> 900 @0-60f"}}"##,
    "easing"
)]
#[case::text_hold(
    r##"{"id":"title","text":"Launch Week","font":"Inter Variable 96 700","fill":"#FFFFFF","at":"center center","span":"0s-0.3s"}"##,
    "text_hold"
)]
#[case::crowd(&sliding(5), "crowd")]
#[case::contrast(
    r##"{"id":"title","text":"Launch Week","font":"Inter Variable 96 700","fill":"#262626","at":"center center"}"##,
    "contrast"
)]
#[case::flash(&blinking(), "flash")]
fn each_planted_defect_trips_its_rule_and_no_other(#[case] layers: &str, #[case] rule: &str) {
    assert_eq!(tripped(layers), BTreeSet::from([rule.to_string()]));
}

#[test]
fn the_same_scenes_without_their_defects_trip_nothing() {
    for calm in [
        r##"{"id":"card","rect":"300x100","fill":"#FFFFFF","at":"center center"}"##,
        r##"{"id":"card","rect":"100x100","fill":"#FFFFFF","at":"300 center","anim":{"x":"300 -> 500 @0-60f out-cubic"}}"##,
        r##"{"id":"title","text":"Launch Week","font":"Inter Variable 96 700","fill":"#FFFFFF","at":"center center"}"##,
        &sliding(4),
    ] {
        assert_eq!(tripped(calm), BTreeSet::new(), "{calm}");
    }
}
