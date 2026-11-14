//! Unit tests for `src/text.rs`.

use super::*;
use rstest::rstest;

fn font(text: &str) -> Font {
    text.parse().unwrap()
}

/// Characters, not bytes, set the width.
#[rstest]
#[case::arabic("مرحبا", 5.0)]
#[case::emoji("🎬🎬", 2.0)]
#[case::empty("", 0.0)]
fn width_is_0_55_em_per_character(#[case] text: &str, #[case] chars: f64) {
    let f = font("Inter 100");
    assert_eq!(ApproxTextMeasure.measure(text, &f).0, chars * 0.55 * 100.0);
}

#[test]
fn height_is_one_point_two_times_the_size() {
    let f = font("Inter 50");
    assert_eq!(ApproxTextMeasure.measure("x", &f).1, 60.0);
}
