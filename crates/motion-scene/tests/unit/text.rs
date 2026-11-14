//! Unit tests for `src/text.rs`.

use super::*;

fn font(text: &str) -> Font {
    text.parse().unwrap()
}

#[test]
fn measures_characters_not_bytes() {
    let f = font("Inter 100");
    assert_eq!(ApproxTextMeasure.measure("مرحبا", &f).0, 5.0 * 0.55 * 100.0);
    assert_eq!(ApproxTextMeasure.measure("🎬🎬", &f).0, 2.0 * 0.55 * 100.0);
}

#[test]
fn height_is_one_point_two_times_the_size() {
    let f = font("Inter 50");
    assert_eq!(ApproxTextMeasure.measure("x", &f).1, 60.0);
}

#[test]
fn empty_text_has_zero_width() {
    let f = font("Inter 50");
    assert_eq!(ApproxTextMeasure.measure("", &f).0, 0.0);
}
