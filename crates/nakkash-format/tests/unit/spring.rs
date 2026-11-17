//! Unit tests for `src/spring.rs`.

use super::*;

fn spring(bounce: f64) -> Spring {
    Spring {
        bounce,
        perceived: None,
    }
}

#[test]
fn a_spring_starts_at_zero_and_ends_exactly_at_one() {
    for bounce in [-0.9, -0.5, 0.0, 0.3, 0.7, 0.95] {
        let motion = spring(bounce).motion();
        assert!(motion.progress(0.0).abs() < 1e-15, "{bounce}");
        assert!((motion.progress(1.0) - 1.0).abs() < 1e-15, "{bounce}");
    }
}

#[test]
fn the_settle_time_is_the_last_time_the_spring_leaves_rest() {
    for bounce in [-0.9, -0.5, 0.0, 0.2, 0.5, 0.8, 0.95] {
        let motion = spring(bounce).motion();
        let settle = motion.settle;
        // Just before it, the motion is still at least the threshold away;
        // after it, never again.
        let before = (1..=200)
            .map(|i| settle * (1.0 - f64::from(i) * 1e-4))
            .any(|t| motion.residual(t).abs() >= REST);
        assert!(before, "{bounce}: nothing moves just before {settle}");
        let after = (0..=20_000).all(|i| {
            let t = settle * (1.0 + f64::from(i) * 1e-3);
            motion.residual(t).abs() < REST
        });
        assert!(after, "{bounce}: it moves again after {settle}");
    }
}

#[test]
fn bounce_sets_the_overshoot_and_none_has_none() {
    let peak = |bounce: f64| {
        let motion = spring(bounce).motion();
        (0..=1000)
            .map(|i| motion.progress(f64::from(i) / 1000.0))
            .fold(f64::MIN, f64::max)
    };
    assert!(
        peak(0.0) <= 1.0 + 1e-12,
        "critical damping never overshoots"
    );
    assert!(peak(-0.5) <= 1.0 + 1e-12, "overdamped never overshoots");
    // An underdamped spring overshoots by e^(-ζπ/√(1-ζ²)).
    let zeta: f64 = 0.5;
    let expected = 1.0 + (-zeta * std::f64::consts::PI / (1.0 - zeta * zeta).sqrt()).exp();
    assert!(
        (peak(0.5) - expected).abs() < 2e-3,
        "{} vs {expected}",
        peak(0.5)
    );
    assert!(peak(0.8) > peak(0.5));
}

#[test]
fn the_settle_time_scales_with_the_perceived_duration_in_its_unit() {
    let frames = Spring {
        bounce: 0.3,
        perceived: Some(Time::Frames(12.0)),
    };
    let seconds = Spring {
        bounce: 0.3,
        perceived: Some(Time::Seconds(1.0)),
    };
    let (Time::Frames(f), Time::Seconds(s)) = (frames.settle(), seconds.settle()) else {
        panic!("the settle time keeps the perceived duration's unit");
    };
    assert!((f / 12.0 - s).abs() < 1e-12);
    // More bounce rings longer; critical damping settles fastest of these.
    let settle = |bounce| match spring(bounce).settle() {
        Time::Seconds(s) => s,
        Time::Frames(_) => unreachable!(),
    };
    assert!(settle(0.6) > settle(0.3));
    assert!(settle(-0.5) > settle(0.0));
}
