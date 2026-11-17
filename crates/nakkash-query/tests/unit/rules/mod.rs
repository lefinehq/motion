//! Unit tests for `src/rules/mod.rs`.

use super::*;
use rstest::rstest;

const AREA: (f64, f64) = (0.0, 1000.0);
const AIM: (f64, f64) = (50.0, 950.0);

#[rstest]
#[case::inside_the_aim_does_not_move((100.0, 200.0), 0.0)]
#[case::past_the_high_edge_is_clamped_into_the_aim((900.0, 1030.0), -80.0)]
#[case::past_the_low_edge_is_clamped_into_the_aim((-10.0, 90.0), 60.0)]
#[case::too_big_for_the_aim_is_clamped_into_the_area((30.0, 1010.0), -10.0)]
#[case::too_big_for_the_area_does_not_move((-100.0, 1100.0), 0.0)]
#[case::whole_pixels((900.0, 950.5), -1.0)]
fn a_sweep_moves(#[case] sweep: (f64, f64), #[case] expected: f64) {
    assert_eq!(axis_shift(sweep, AREA, AIM), expected);
}

#[test]
fn exact_pixels_when_rounding_would_leave_the_target() {
    // 899.8 wide in a 900 aim: rounding 0.6 up to 1 would push the
    // other edge out, so the exact move is kept.
    let s = axis_shift((49.4, 949.2), AREA, AIM);
    assert!((s - 0.6).abs() < 1e-9, "{s}");
}

#[test]
fn both_axes_move_at_once() {
    let s = shift(
        Rect::new(900.0, -10.0, 1030.0, 90.0),
        Rect::new(0.0, 0.0, 1000.0, 500.0),
        Rect::new(50.0, 25.0, 950.0, 475.0),
    );
    assert_eq!(s, Vec2::new(-80.0, 35.0));
}

#[test]
fn each_rule_has_its_own_code_and_is_suppressed_only_by_an_earlier_rule() {
    for (i, rule) in RULES.iter().enumerate() {
        assert!(
            RULES[..i].iter().all(|earlier| earlier.code != rule.code),
            "{}",
            rule.code
        );
        if let Some(by) = rule.suppressed_by {
            assert!(
                RULES[..i].iter().any(|earlier| earlier.code == by),
                "{} is suppressed by {by}, which must run before it",
                rule.code
            );
        }
    }
}
