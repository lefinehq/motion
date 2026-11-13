//! Unit tests for `src/easing.rs`.

use super::*;

#[test]
fn easings_hit_endpoints_and_known_midpoints() {
    for e in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
    ] {
        assert!(sample(e, 0.0).abs() < 1e-9 && (sample(e, 1.0) - 1.0).abs() < 1e-9);
    }
    assert_eq!(sample(Easing::Linear, 0.25), 0.25);
    assert!((sample(Easing::EaseOut, 0.5) - 0.684).abs() < 1e-3);
    assert!((sample(Easing::EaseInOut, 0.5) - 0.5).abs() < 1e-6);
    assert!(
        (0..=100)
            .map(|i| sample(Easing::EaseIn, i as f64 / 100.0))
            .collect::<Vec<_>>()
            .windows(2)
            .all(|w| w[1] >= w[0])
    );
    assert_eq!(sample(Easing::EaseOut, -1.0), 0.0); // clamps
}

#[test]
fn clamps_above_one() {
    assert_eq!(sample(Easing::EaseIn, 2.0), 1.0);
}

#[test]
fn ease_in_is_slow_at_first_and_ease_out_fast() {
    assert!(sample(Easing::EaseIn, 0.25) < 0.25);
    assert!(sample(Easing::EaseOut, 0.25) > 0.25);
}

#[test]
fn named_easings_equal_their_cubic_curves() {
    for (named, cubic) in [
        (Easing::EaseIn, Easing::Cubic(0.42, 0.0, 1.0, 1.0)),
        (Easing::EaseOut, Easing::Cubic(0.0, 0.0, 0.58, 1.0)),
        (Easing::EaseInOut, Easing::Cubic(0.42, 0.0, 0.58, 1.0)),
    ] {
        for p in [0.1, 0.25, 0.5, 0.9] {
            assert_eq!(sample(named, p), sample(cubic, p));
        }
    }
}

#[test]
fn a_cubic_curve_can_overshoot_and_still_ends_at_one() {
    let back = Easing::Cubic(0.3, 0.0, 0.3, 1.6);
    let peak = (1..100)
        .map(|i| sample(back, i as f64 / 100.0))
        .fold(f64::MIN, f64::max);
    assert!(peak > 1.05, "{peak}");
    assert_eq!(sample(back, 1.0), 1.0);
    assert_eq!(sample(back, 0.0), 0.0);
}
