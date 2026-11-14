//! Unit tests for `src/track.rs`.

use super::*;
use motion_format::Curve;
use proptest::prelude::*;

fn n(v: f64) -> Value {
    Value::Number(v)
}

fn p(x: f64, y: f64) -> Value {
    Value::Point([x, y])
}

fn key(frame: f64, value: Value, curve: Curve) -> KeySpec {
    KeySpec {
        easing: Easing::All(curve),
        ..KeySpec::linear(frame, value)
    }
}

/// A number track at 10 fps.
fn numbers(keys: Vec<KeySpec>) -> Track {
    Track::new(ValueType::Number, keys, 10.0, false)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn holds_the_first_value_before_and_the_last_after() {
    let t = numbers(vec![
        KeySpec::linear(10.0, n(2.0)),
        KeySpec::linear(20.0, n(4.0)),
    ]);
    assert_eq!(t.sample(0.0), n(2.0));
    assert_eq!(t.sample(10.0), n(2.0));
    assert_eq!(t.sample(20.0), n(4.0));
    assert_eq!(t.sample(99.0), n(4.0));
    assert_eq!(t.sample(15.0), n(3.0));
}

#[test]
fn the_left_keys_curve_applies() {
    let t = numbers(vec![
        key(0.0, n(0.0), Curve::Hold),
        KeySpec::linear(10.0, n(1.0)),
    ]);
    assert_eq!(t.sample(9.9), n(0.0));
    assert_eq!(t.sample(10.0), n(1.0));
}

#[test]
fn the_last_of_same_frame_keys_wins_from_that_frame() {
    let t = numbers(vec![
        KeySpec::linear(0.0, n(0.0)),
        KeySpec::linear(5.0, n(1.0)),
        KeySpec::linear(5.0, n(9.0)),
    ]);
    assert_eq!(t.sample(4.0), n(0.8));
    assert_eq!(t.sample(5.0), n(9.0));
    assert_eq!(t.sample(6.0), n(9.0));
}

#[test]
fn a_one_key_track_is_constant() {
    let t = Track::constant(n(7.0));
    assert!(t.is_constant());
    assert_eq!(t.value_type(), ValueType::Number);
    assert_eq!((t.sample(-5.0), t.sample(1e9)), (n(7.0), n(7.0)));
    assert!(
        !numbers(vec![
            KeySpec::linear(0.0, n(0.0)),
            KeySpec::linear(1.0, n(1.0))
        ])
        .is_constant()
    );
}

#[test]
fn equal_keys_with_a_speed_still_move() {
    // Leaves at 10 units/s and comes back at -10: an excursion a normalised curve cannot hold.
    let speed = Curve::Speed {
        out_speed: 10.0,
        out_reach: 0.5,
        in_speed: -10.0,
        in_reach: 0.5,
    };
    let t = numbers(vec![key(0.0, n(0.0), speed), KeySpec::linear(10.0, n(0.0))]);
    assert!(!t.is_constant());
    let mid = t.sample(5.0).as_number().unwrap();
    // Each handle rises 10 * 0.5 * 1 s = 5; the curve peaks at 3/4 of that.
    assert!(close(mid, 3.75), "{mid}");
    assert_eq!(t.sample(10.0), n(0.0));
}

#[test]
fn a_normalised_curve_scales_with_the_change() {
    let ease = Curve::named("ease-out").unwrap();
    let t = numbers(vec![
        key(0.0, n(100.0), ease),
        KeySpec::linear(10.0, n(300.0)),
    ]);
    let at = |f| t.sample(f).as_number().unwrap();
    // ease-out at half time is about 0.684 of the way.
    assert!(
        (at(5.0) - (100.0 + 200.0 * 0.684)).abs() < 0.2,
        "{}",
        at(5.0)
    );
}

#[test]
fn each_component_can_take_its_own_curve() {
    let keys = vec![
        KeySpec {
            easing: Easing::Each(vec![Curve::Hold, Curve::Linear]),
            ..KeySpec::linear(0.0, p(0.0, 0.0))
        },
        KeySpec::linear(10.0, p(10.0, 10.0)),
    ];
    let t = Track::new(ValueType::Point, keys, 10.0, false);
    assert_eq!(t.sample(5.0), p(0.0, 5.0));
}

#[test]
fn a_spatial_track_follows_its_bent_path_at_constant_speed() {
    // A quarter turn of radius 100 made of one bézier, at a linear pace.
    let k = 0.552_284_75 * 100.0;
    let keys = vec![
        KeySpec {
            path: Some([[k, 0.0], [0.0, -k]]),
            ..KeySpec::linear(0.0, p(0.0, 0.0))
        },
        KeySpec::linear(10.0, p(100.0, 100.0)),
    ];
    let t = Track::new(ValueType::Point, keys, 10.0, true);
    let [x, y] = t.sample(5.0).as_point().unwrap();
    // Halfway along the arc is 45° round it: (100 sin 45°, 100 - 100 cos 45°).
    let h = 100.0 * std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (x - h).abs() < 0.05 && (y - (100.0 - h)).abs() < 0.05,
        "{x},{y}"
    );
    // Linear pace: equal times, equal distances along the arc.
    let at = |f: f64| {
        let [x, y] = t.sample(f).as_point().unwrap();
        (x, y)
    };
    let d = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1);
    assert!((d(at(0.0), at(2.0)) - d(at(8.0), at(10.0))).abs() < 0.01);
}

#[test]
fn a_straight_spatial_span_matches_plain_interpolation() {
    let ease = Curve::named("ease-in-out").unwrap();
    let keys = || {
        vec![
            key(0.0, p(0.0, 0.0), ease),
            KeySpec::linear(10.0, p(30.0, 40.0)),
        ]
    };
    let spatial = Track::new(ValueType::Point, keys(), 10.0, true);
    let plain = Track::new(ValueType::Point, keys(), 10.0, false);
    for f in [1.0, 3.0, 5.0, 7.5, 9.0] {
        let (a, b) = (
            spatial.sample(f).as_point().unwrap(),
            plain.sample(f).as_point().unwrap(),
        );
        assert!(close(a[0], b[0]) && close(a[1], b[1]), "{f}: {a:?} {b:?}");
    }
}

#[test]
fn a_spatial_speed_is_pixels_per_second_along_the_path() {
    // 50 px in 1 s, leaving at 150 px/s over the whole span and arriving at 0.
    let speed = Curve::Speed {
        out_speed: 150.0,
        out_reach: 1.0 / 3.0,
        in_speed: 0.0,
        in_reach: 1.0 / 3.0,
    };
    let t = Track::new(
        ValueType::Point,
        vec![
            key(0.0, p(0.0, 0.0), speed),
            KeySpec::linear(10.0, p(30.0, 40.0)),
        ],
        10.0,
        true,
    );
    let [x, y] = t.sample(0.01).as_point().unwrap();
    // 0.001 s in at 150 px/s is 0.15 px along the 3-4-5 line.
    assert!((x.hypot(y) - 0.15).abs() < 1e-3, "{}", x.hypot(y));
    assert!((x / y - 0.75).abs() < 1e-9);
}

#[test]
fn an_overshooting_spatial_ease_carries_on_past_the_end() {
    let back = Curve::Bezier(0.3, 0.0, 0.3, 1.6);
    let t = Track::new(
        ValueType::Point,
        vec![
            key(0.0, p(0.0, 0.0), back),
            KeySpec::linear(10.0, p(100.0, 0.0)),
        ],
        10.0,
        true,
    );
    let beyond = (1..10)
        .map(|f| t.sample(f64::from(f)).as_point().unwrap()[0])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(beyond > 100.0, "{beyond}");
    let still = Track::new(
        ValueType::Point,
        vec![
            key(0.0, p(5.0, 5.0), back),
            KeySpec::linear(10.0, p(5.0, 5.0)),
        ],
        10.0,
        true,
    );
    assert!(still.is_constant());
    assert_eq!(still.sample(4.0), p(5.0, 5.0));
}

#[test]
fn keys_are_sorted_stably() {
    let t = numbers(vec![
        KeySpec::linear(5.0, n(1.0)),
        KeySpec::linear(0.0, n(0.0)),
        KeySpec::linear(5.0, n(2.0)),
    ]);
    let frames: Vec<f64> = t.keys().iter().map(|k| k.frame).collect();
    assert_eq!(frames, [0.0, 5.0, 5.0]);
    assert_eq!(t.sample(5.0), n(2.0));
}

#[test]
#[should_panic(expected = "at least one key")]
fn an_empty_track_is_rejected() {
    numbers(Vec::new());
}

#[test]
#[should_panic(expected = "keys of one type")]
fn keys_of_another_type_are_rejected() {
    numbers(vec![KeySpec::linear(0.0, p(0.0, 0.0))]);
}

proptest! {
    #[test]
    fn sampling_never_panics_and_stays_within_the_key_values(
        raw in proptest::collection::vec((-1e4f64..1e4, -1e4f64..1e4, any::<bool>()), 1..8),
        frame in -1e5f64..1e5,
    ) {
        let keys: Vec<KeySpec> = raw
            .iter()
            .map(|&(f, v, hold)| key(f, n(v), if hold { Curve::Hold } else { Curve::Linear }))
            .collect();
        let lo = raw.iter().map(|k| k.1).fold(f64::INFINITY, f64::min);
        let hi = raw.iter().map(|k| k.1).fold(f64::NEG_INFINITY, f64::max);
        let v = numbers(keys).sample(frame).as_number().unwrap();
        // A little slack for the interpolation's round-off.
        prop_assert!(v >= lo - 1e-6 && v <= hi + 1e-6, "{v} outside {lo}..={hi}");
    }

    #[test]
    fn a_spatial_track_never_panics(
        raw in proptest::collection::vec(
            (-1e3f64..1e3, -1e3f64..1e3, -1e3f64..1e3, prop::array::uniform4(-500f64..500.0)),
            1..6,
        ),
        frame in -1e4f64..1e4,
    ) {
        let keys: Vec<KeySpec> = raw
            .iter()
            .map(|&(f, x, y, [a, b, c, d])| KeySpec {
                path: Some([[a, b], [c, d]]),
                ..KeySpec::linear(f, p(x, y))
            })
            .collect();
        let [x, y] = Track::new(ValueType::Point, keys, 25.0, true)
            .sample(frame)
            .as_point()
            .unwrap();
        prop_assert!(x.is_finite() && y.is_finite());
    }
}
