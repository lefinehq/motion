//! Unit tests for `src/oklch.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;

fn close(a: [f64; 4], b: [f64; 4], within: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= within)
}

#[test]
fn white_and_black_are_the_ends_of_lightness() {
    let [l, a, b] = oklab([1.0, 1.0, 1.0]);
    assert!((l - 1.0).abs() < 1e-6 && a.abs() < 1e-6 && b.abs() < 1e-6);
    assert_eq!(oklab([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
}

#[rstest]
#[case([0.901961, 0.223529, 0.27451])]
#[case([0.098039, 0.431373, 1.0])]
#[case([0.5, 0.5, 0.5])]
#[case([1.0, 0.85, 0.0])]
fn a_colour_comes_back_from_oklch(#[case] rgb: [f64; 3]) {
    let back = Oklch::from_srgb(rgb).to_srgb();
    assert!(close(
        [back[0], back[1], back[2], 1.0],
        [rgb[0], rgb[1], rgb[2], 1.0],
        1e-6
    ));
}

#[rstest]
#[case::near([0.901961, 0.223529, 0.27451, 1.0], [0.89, 0.23, 0.29, 1.0])]
#[case::darker([0.901961, 0.223529, 0.27451, 1.0], [0.6, 0.1, 0.15, 1.0])]
#[case::translucent([0.098039, 0.431373, 1.0, 1.0], [0.098039, 0.431373, 1.0, 0.5])]
#[case::grey_to_grey([0.5, 0.5, 0.5, 1.0], [0.52, 0.52, 0.52, 1.0])]
#[case::grey_to_colour([0.5, 0.5, 0.5, 1.0], [0.55, 0.5, 0.48, 1.0])]
fn a_shift_between_two_colours_gives_the_second_back(#[case] from: [f64; 4], #[case] to: [f64; 4]) {
    let shift = Shift::between(from, to);
    let written = Shift::parse(&shift.to_string()).unwrap_or(Shift::NONE);
    assert!(
        close(written.apply(from), to, 2e-6),
        "{shift}: {:?}",
        written.apply(from)
    );
}

#[test]
fn the_same_colour_needs_no_shift() {
    let color = [0.2, 0.4, 0.6, 1.0];
    assert!(Shift::between(color, color).is_none());
}

#[test]
fn a_shift_moves_a_new_colour_the_same_way() {
    // Darker than red by the same amount is darker than blue too.
    let red = [0.9, 0.2, 0.25, 1.0];
    let darker = Shift::between(red, [0.7, 0.1, 0.15, 1.0]);
    let blue = Oklch::from_srgb([0.1, 0.4, 0.95]);
    let moved = darker.apply([0.1, 0.4, 0.95, 1.0]);
    let moved = Oklch::from_srgb([moved[0], moved[1], moved[2]]);
    assert!(moved.l < blue.l);
    assert!((moved.h - blue.h).abs() < 15.0);
}

#[rstest]
#[case("l-0.021 c+0.004 h+1.6", Shift { l: -0.021, c: 0.004, h: 1.6, alpha: None })]
#[case("/ 0.5", Shift { alpha: Some(0.5), ..Shift::NONE })]
#[case("l+0.1 / 0", Shift { l: 0.1, alpha: Some(0.0), ..Shift::NONE })]
#[case("h-30", Shift { h: -30.0, ..Shift::NONE })]
fn a_shift_reads_as_written(#[case] text: &str, #[case] shift: Shift) {
    assert_eq!(Shift::parse(text), Some(shift));
    assert_eq!(shift.to_string(), text);
}

#[rstest]
#[case::empty("")]
#[case::words("off")]
#[case::no_sign("l0.1")]
#[case::out_of_order("c+0.1 l+0.1")]
#[case::twice("l+0.1 l+0.2")]
#[case::alpha_too_large("/ 2")]
#[case::price("99 off")]
fn other_text_is_no_shift(#[case] text: &str) {
    assert_eq!(Shift::parse(text), None);
}

#[test]
fn a_shift_stays_inside_srgb() {
    let shift = Shift {
        l: 0.5,
        c: 0.3,
        ..Shift::NONE
    };
    let out = shift.apply([0.9, 0.9, 0.2, 1.0]);
    assert!(out.iter().all(|c| (0.0..=1.0).contains(c)));
}

#[test]
fn distance_is_small_for_near_colours() {
    assert!(distance([0.9, 0.22, 0.27], [0.9, 0.23, 0.28]) < 0.02);
    assert!(distance([0.9, 0.22, 0.27], [0.1, 0.4, 0.95]) > 0.3);
}
