//! Unit tests for `src/fit.rs`.

use super::*;

/// 30 samples a second from 0 to `seconds`.
fn frames(seconds: f64) -> Vec<f64> {
    (0..=(seconds * 30.0) as usize)
        .map(|i| i as f64 / 30.0)
        .collect()
}

fn check(times: &[f64], values: &[f64], tolerance: f64) -> Fitted {
    let fitted = fit(times, values, tolerance);
    for (t, v) in times.iter().zip(values) {
        let e = (value_at(&fitted.keys, *t) - v).abs();
        assert!(e <= tolerance + 1e-9, "{e} at {t}");
    }
    assert!(fitted.max_error <= tolerance + 1e-9);
    fitted
}

#[test]
fn a_constant_is_two_keys() {
    let t = frames(2.0);
    let fitted = check(&t, &vec![5.0; t.len()], 0.01);
    assert_eq!(fitted.keys.len(), 2);
}

#[test]
fn a_cubic_in_time_is_two_keys() {
    let t = frames(2.0);
    let v: Vec<f64> = t
        .iter()
        .map(|t| 3.0 * t * t * t - 2.0 * t * t + t + 1.0)
        .collect();
    let fitted = check(&t, &v, 1e-6);
    assert_eq!(fitted.keys.len(), 2);
}

#[test]
fn an_ease_out_is_a_few_keys() {
    let t = frames(1.0);
    let v: Vec<f64> = t
        .iter()
        .map(|t| 500.0 * 1.000_976 * (1.0 - 2f64.powf(-10.0 * t)))
        .collect();
    for tolerance in [0.05, 0.1, 0.25, 0.5] {
        let fitted = check(&t, &v, tolerance);
        eprintln!(
            "ease out within {tolerance}: {} keys for {} frames",
            fitted.keys.len(),
            t.len()
        );
    }
    let fitted = check(&t, &v, 0.1);
    assert!(
        fitted.keys.len() <= t.len() / 2,
        "{} keys",
        fitted.keys.len()
    );
}

#[test]
fn a_bounce_needs_more_keys_but_far_fewer_than_frames() {
    let t = frames(2.0);
    let v: Vec<f64> = t
        .iter()
        .map(|t| 100.0 + 40.0 * (3.0 * t * std::f64::consts::TAU).sin() / (4.0 * t).exp())
        .collect();
    let fitted = check(&t, &v, 0.05);
    assert!(
        fitted.keys.len() < t.len() / 3,
        "{} keys for {} frames",
        fitted.keys.len(),
        t.len()
    );
    eprintln!("bounce: {} keys for {} frames", fitted.keys.len(), t.len());
}

#[test]
fn steps_hold() {
    let t = frames(1.0);
    let v: Vec<f64> = t.iter().map(|t| (t * 4.0).floor() * 25.0).collect();
    let fitted = check(&t, &v, 0.01);
    assert_eq!(fitted.keys.iter().filter(|k| k.hold).count(), 4);
    assert!(fitted.keys.len() <= 9, "{} keys", fitted.keys.len());
}

#[test]
fn a_single_sample_is_one_key() {
    let fitted = fit(&[0.5], &[3.0], 0.1);
    assert_eq!(fitted.keys.len(), 1);
    assert_eq!(value_at(&fitted.keys, 9.0), 3.0);
}

#[test]
fn noise_stays_within_the_tolerance() {
    let t = frames(3.0);
    let v: Vec<f64> = (0..t.len())
        .map(|i| ((i * 7919) % 101) as f64 / 10.0)
        .collect();
    check(&t, &v, 0.2);
}
