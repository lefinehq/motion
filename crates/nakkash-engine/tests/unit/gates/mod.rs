//! Unit tests for `src/gates/mod.rs`.

use super::*;
use rstest::rstest;

#[rstest]
#[case::black([0, 0, 0], 0.0)]
#[case::white([255, 255, 255], 1.0)]
#[case::mid_grey([119, 119, 119], 0.184)]
fn luminance_is_wcags(#[case] rgb: [u8; 3], #[case] expected: f64) {
    assert!(
        (luminance(rgb) - expected).abs() < 1e-3,
        "{}",
        luminance(rgb)
    );
}

#[test]
fn a_clear_px_shows_black() {
    assert_eq!(over_black(&[255, 255, 255, 0]), [0, 0, 0]);
    assert_eq!(over_black(&[255, 128, 0, 255]), [255, 128, 0]);
}
