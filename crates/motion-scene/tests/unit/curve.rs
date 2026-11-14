//! Unit tests for `src/curve.rs`.

use super::*;

/// A curve from the table, by name.
fn named(name: &str) -> Curve {
    Curve::named(name).unwrap()
}

#[test]
fn easings_hit_endpoints_and_known_midpoints() {
    for e in [
        Curve::Linear,
        named("ease-in"),
        named("ease-out"),
        named("ease-in-out"),
    ] {
        assert!(sample(e, 0.0).abs() < 1e-9 && (sample(e, 1.0) - 1.0).abs() < 1e-9);
    }
    assert_eq!(sample(Curve::Linear, 0.25), 0.25);
    assert!((sample(named("ease-out"), 0.5) - 0.684).abs() < 1e-3);
    assert!((sample(named("ease-in-out"), 0.5) - 0.5).abs() < 1e-6);
    assert!(
        (0..=100)
            .map(|i| sample(named("ease-in"), i as f64 / 100.0))
            .collect::<Vec<_>>()
            .windows(2)
            .all(|w| w[1] >= w[0])
    );
    assert_eq!(sample(named("ease-out"), -1.0), 0.0); // clamps
}

#[test]
fn clamps_above_one() {
    assert_eq!(sample(named("ease-in"), 2.0), 1.0);
}

#[test]
fn ease_in_is_slow_at_first_and_ease_out_fast() {
    assert!(sample(named("ease-in"), 0.25) < 0.25);
    assert!(sample(named("ease-out"), 0.25) > 0.25);
}

#[test]
fn named_easings_equal_their_cubic_curves() {
    for (named, cubic) in [
        (named("ease-in"), Curve::Bezier(0.42, 0.0, 1.0, 1.0)),
        (named("ease-out"), Curve::Bezier(0.0, 0.0, 0.58, 1.0)),
        (named("ease-in-out"), Curve::Bezier(0.42, 0.0, 0.58, 1.0)),
    ] {
        for p in [0.1, 0.25, 0.5, 0.9] {
            assert_eq!(sample(named, p), sample(cubic, p));
        }
    }
}

#[test]
fn a_cubic_curve_can_overshoot_and_still_ends_at_one() {
    let back = Curve::Bezier(0.3, 0.0, 0.3, 1.6);
    let peak = (1..100)
        .map(|i| sample(back, i as f64 / 100.0))
        .fold(f64::MIN, f64::max);
    assert!(peak > 1.05, "{peak}");
    assert_eq!(sample(back, 1.0), 1.0);
    assert_eq!(sample(back, 0.0), 0.0);
}

#[test]
fn hold_stays_at_zero_until_the_end() {
    assert_eq!(sample(Curve::Hold, 0.0), 0.0);
    assert_eq!(sample(Curve::Hold, 0.99), 0.0);
    assert_eq!(sample(Curve::Hold, 1.0), 1.0);
    assert_eq!(sample(Curve::Hold, 2.0), 1.0);
}

#[test]
fn every_named_bezier_equals_its_cubic_form() {
    for (name, curve) in motion_format::NAMED_CURVES {
        let Curve::Bezier(a, b, c, d) = *curve else {
            continue;
        };
        let cubic = format!("cubic({a},{b},{c},{d})").parse::<Curve>().unwrap();
        for p in [0.1, 0.25, 0.5, 0.9] {
            assert_eq!(sample(*curve, p), sample(cubic, p), "{name}");
        }
    }
}
