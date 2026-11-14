//! Unit tests for `src/rules/mod.rs`.

use super::*;

const AREA: (f64, f64) = (0.0, 1000.0);
const AIM: (f64, f64) = (50.0, 950.0);

#[test]
fn a_sweep_inside_the_aim_does_not_move() {
    assert_eq!(axis_shift((100.0, 200.0), AREA, AIM), 0.0);
}

#[test]
fn a_sweep_past_either_edge_is_clamped_into_the_aim() {
    assert_eq!(axis_shift((900.0, 1030.0), AREA, AIM), -80.0);
    assert_eq!(axis_shift((-10.0, 90.0), AREA, AIM), 60.0);
}

#[test]
fn a_sweep_too_big_for_the_aim_is_clamped_into_the_area() {
    assert_eq!(axis_shift((30.0, 1010.0), AREA, AIM), -10.0);
}

#[test]
fn a_sweep_too_big_for_the_area_does_not_move() {
    assert_eq!(axis_shift((-100.0, 1100.0), AREA, AIM), 0.0);
}

#[test]
fn whole_pixels_unless_rounding_would_leave_the_target() {
    assert_eq!(axis_shift((900.0, 950.5), AREA, AIM), -1.0);
    // 899.8 wide in a 900 aim: rounding 0.6 up to 1 would push the
    // other edge out, so the exact move is kept.
    let s = axis_shift((49.4, 949.2), AREA, AIM);
    assert!((s - 0.6).abs() < 1e-9, "{s}");
}

#[test]
fn both_axes_move_at_once() {
    let s = shift(
        (900.0, -10.0, 1030.0, 90.0),
        (0.0, 0.0, 1000.0, 500.0),
        (50.0, 25.0, 950.0, 475.0),
    );
    assert_eq!(s, (-80.0, 35.0));
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
