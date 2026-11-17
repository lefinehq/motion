//! Unit tests for `src/curve.rs`.

use super::*;

/// Eased progress: `curve` over a span from 0 to 1 lasting one second.
fn sample(curve: Curve, p: f64) -> f64 {
    Ease::from_curve(curve, 1.0, 1.0).sample(0.0, 1.0, p)
}

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
    for (name, curve) in nakkash_format::NAMED_CURVES {
        let Curve::Bezier(a, b, c, d) = *curve else {
            continue;
        };
        let cubic = format!("cubic({a},{b},{c},{d})").parse::<Curve>().unwrap();
        for p in [0.1, 0.25, 0.5, 0.9] {
            assert_eq!(sample(*curve, p), sample(cubic, p), "{name}");
        }
    }
}

#[test]
fn a_normalised_curve_becomes_handles_scaled_by_the_change() {
    assert_eq!(
        Ease::from_curve(Curve::Bezier(0.2, 0.1, 0.6, 1.0), -50.0, 2.0),
        Ease::Bezier {
            out: (0.2, -5.0),
            into: (0.4, 0.0)
        }
    );
}

#[test]
fn a_speed_becomes_handles_in_value_units() {
    let speed = Curve::Speed {
        out_speed: 300.0,
        out_reach: 0.25,
        in_speed: -40.0,
        in_reach: 0.5,
    };
    // Over 2 s: the out handle spans 0.5 s at 300/s, the in handle 1 s at -40/s.
    assert_eq!(
        Ease::from_curve(speed, 123.0, 2.0),
        Ease::Bezier {
            out: (0.25, 150.0),
            into: (0.5, -40.0)
        }
    );
}

#[test]
fn a_bezier_ease_passes_through_its_ends_and_is_smooth_between() {
    // The arriving handle rises by -10, so it sits 10 above the end: a bump.
    let ease = Ease::Bezier {
        out: (0.5, 10.0),
        into: (0.5, -10.0),
    };
    assert_eq!(ease.sample(0.0, 0.0, 0.0), 0.0);
    assert_eq!(ease.sample(0.0, 0.0, 1.0), 0.0);
    // Symmetric handles that rise and come back peak at the middle, at 3/4 of the rise.
    assert!((ease.sample(0.0, 0.0, 0.5) - 7.5).abs() < 1e-9);
}

#[test]
fn linear_and_hold_ignore_the_span_length() {
    assert_eq!(Ease::from_curve(Curve::Linear, 5.0, 9.0), Ease::Linear);
    assert_eq!(Ease::from_curve(Curve::Hold, 5.0, 9.0), Ease::Hold);
    assert_eq!(Ease::Linear.sample(10.0, 20.0, 0.25), 12.5);
    assert_eq!(Ease::Hold.sample(10.0, 20.0, 0.99), 10.0);
}

/// A family's curve by its token.
fn family(token: &str) -> Curve {
    token.parse().unwrap()
}

#[test]
fn every_family_meets_both_ends_in_every_shape() {
    for (s, _) in nakkash_format::SHAPES {
        for (f, _) in nakkash_format::FAMILIES {
            let curve = family(&format!("{s}-{f}"));
            // Circ meets its ends upright, growing as the square root of
            // the distance, so a hair from the end is a thousandth of that.
            for (p, want) in [(1e-12, 0.0), (1.0 - 1e-12, 1.0)] {
                let got = sample(curve, p);
                assert!((got - want).abs() < 1e-5, "{s}-{f} at {p}: {got}");
            }
        }
    }
}

#[test]
fn families_give_their_known_values() {
    let at = |token: &str, p: f64| sample(family(token), p);
    // Out-expo: 1 - (2^(10(1-p)) - 1)/1023.
    assert!((at("out-expo", 0.5) - (1.0 - 31.0 / 1023.0)).abs() < 1e-12);
    assert!((at("in-out-quint", 0.25) - 0.015_625).abs() < 1e-12);
    assert!((at("out-bounce", 0.5) - 0.765_625).abs() < 1e-12);
    assert!((at("in-sine", 0.5) - (1.0 - (std::f64::consts::FRAC_PI_4).cos())).abs() < 1e-12);
    assert!(at("in-back", 0.3) < 0.0, "back winds up first");
    assert!(at("out-back", 0.7) > 1.0, "and overshoots on the way out");
    assert!(at("out-elastic", 0.15) > 1.3, "elastic swings past the end");
    assert!((at("in(1.5)", 0.25) - 0.125).abs() < 1e-12);
}

#[test]
fn shapes_mirror_one_another() {
    for (f, _) in nakkash_format::FAMILIES {
        for i in 1..20 {
            let p = f64::from(i) / 20.0;
            let in_ = sample(family(&format!("in-{f}")), p);
            let out = sample(family(&format!("out-{f}")), 1.0 - p);
            assert!((in_ - (1.0 - out)).abs() < 1e-12, "{f} at {p}");
            let in_out = sample(family(&format!("in-out-{f}")), p);
            let mirrored = sample(family(&format!("in-out-{f}")), 1.0 - p);
            assert!(
                (in_out - (1.0 - mirrored)).abs() < 1e-12,
                "in-out-{f} at {p}"
            );
        }
    }
}

#[test]
fn families_that_do_not_overshoot_never_run_backwards() {
    for f in ["quad", "cubic", "quart", "quint", "sine", "circ", "expo"] {
        for s in ["in", "out", "in-out", "out-in"] {
            let values: Vec<f64> = (0..=200)
                .map(|i| sample(family(&format!("{s}-{f}")), f64::from(i) / 200.0))
                .collect();
            assert!(values.windows(2).all(|w| w[1] >= w[0]), "{s}-{f}");
            assert!(values.iter().all(|v| (0.0..=1.0).contains(v)), "{s}-{f}");
        }
    }
}

#[test]
fn steps_jump_at_the_end_or_the_start_of_each_step() {
    let end = Curve::Steps {
        count: 4,
        start: false,
    };
    let start = Curve::Steps {
        count: 4,
        start: true,
    };
    assert_eq!(sample(end, 0.2), 0.0);
    assert_eq!(sample(end, 0.25), 0.25);
    assert_eq!(sample(end, 0.99), 0.75);
    assert_eq!(sample(end, 1.0), 1.0);
    assert_eq!(sample(start, 0.01), 0.25);
    assert_eq!(sample(start, 0.75), 0.75);
    assert_eq!(sample(start, 0.76), 1.0);
}

#[test]
fn a_family_scales_with_the_change_so_equal_keys_hold() {
    let ease = Ease::from_curve(family("out-elastic"), 0.0, 1.0);
    assert!(ease.is_still());
    assert_eq!(ease.sample(5.0, 5.0, 0.4), 5.0);
}
