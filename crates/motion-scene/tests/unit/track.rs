//! Unit tests for `src/track.rs`.

use super::*;
use proptest::prelude::*;
use rstest::rstest;

fn key(frame: f64, value: f64, curve: Curve) -> Key {
    Key {
        frame,
        value,
        curve,
    }
}

fn linear(frame: f64, value: f64) -> Key {
    key(frame, value, Curve::Linear)
}

/// Holds the first value before the keys and the last after them, and eases between.
#[rstest]
#[case::before(0.0, 2.0)]
#[case::on_the_first(10.0, 2.0)]
#[case::between(15.0, 3.0)]
#[case::on_the_last(20.0, 4.0)]
#[case::after(99.0, 4.0)]
fn samples_a_two_key_track(#[case] frame: f64, #[case] value: f64) {
    let t = Track::new(vec![linear(10.0, 2.0), linear(20.0, 4.0)]);
    assert_eq!(t.sample(frame), value);
}

#[test]
fn the_left_keys_curve_applies() {
    let t = Track::new(vec![key(0.0, 0.0, Curve::Hold), linear(10.0, 1.0)]);
    assert_eq!(t.sample(9.9), 0.0);
    assert_eq!(t.sample(10.0), 1.0);
}

#[test]
fn the_last_of_same_frame_keys_wins_from_that_frame() {
    let t = Track::new(vec![linear(0.0, 0.0), linear(5.0, 1.0), linear(5.0, 9.0)]);
    assert_eq!(t.sample(4.0), 0.8);
    assert_eq!(t.sample(5.0), 9.0);
    assert_eq!(t.sample(6.0), 9.0);
}

#[test]
fn a_one_key_track_is_constant() {
    let t = Track::constant(7.0);
    assert!(t.is_constant());
    assert_eq!(
        (t.sample(-5.0), t.sample(0.0), t.sample(1e9)),
        (7.0, 7.0, 7.0)
    );
    assert!(!Track::new(vec![linear(0.0, 0.0), linear(1.0, 1.0)]).is_constant());
}

#[test]
fn keys_are_sorted_stably() {
    let t = Track::new(vec![linear(5.0, 1.0), linear(0.0, 0.0), linear(5.0, 2.0)]);
    let frames: Vec<f64> = t.keys().iter().map(|k| k.frame).collect();
    assert_eq!(frames, [0.0, 5.0, 5.0]);
    assert_eq!(t.sample(5.0), 2.0);
}

#[test]
#[should_panic(expected = "at least one key")]
fn an_empty_track_is_rejected() {
    Track::new(Vec::new());
}

proptest! {
    #[test]
    fn sampling_never_panics_and_stays_within_the_key_values(
        raw in proptest::collection::vec((-1e4f64..1e4, -1e4f64..1e4, any::<bool>()), 1..8),
        frame in -1e5f64..1e5,
    ) {
        let keys: Vec<Key> = raw
            .iter()
            .map(|&(f, v, hold)| key(f, v, if hold { Curve::Hold } else { Curve::Linear }))
            .collect();
        let lo = keys.iter().map(|k| k.value).fold(f64::INFINITY, f64::min);
        let hi = keys.iter().map(|k| k.value).fold(f64::NEG_INFINITY, f64::max);
        let v = Track::new(keys).sample(frame);
        // A little slack for the interpolation's round-off.
        prop_assert!(v >= lo - 1e-6 && v <= hi + 1e-6, "{v} outside {lo}..={hi}");
    }
}
