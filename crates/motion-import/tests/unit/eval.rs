//! Unit tests for `src/eval.rs`: a property's value from its keys.

use super::*;
use crate::model::Spatial;

/// A key at `time` holding `value`, eased on both sides at `speed` with
/// influence `reach`, or linear when `reach` is `None`.
fn key(time: f64, value: Value, speed: f64, reach: Option<f64>) -> Keyframe {
    let interpolation = match reach {
        Some(_) => Interpolation::Bezier,
        None => Interpolation::Linear,
    };
    let ease = vec![Ease {
        speed,
        influence: reach.unwrap_or(0.0),
    }];
    Keyframe {
        time,
        value,
        in_interpolation: interpolation,
        out_interpolation: interpolation,
        in_ease: ease.clone(),
        out_ease: ease,
        spatial: None,
        continuous: false,
        auto_bezier: false,
        roving: false,
    }
}

fn number(stream: &Stream, time: f64) -> f64 {
    match stream.value_at(time) {
        Value::Number(n) => n,
        other => panic!("not a number: {other:?}"),
    }
}

#[test]
fn a_static_value_holds() {
    let stream = Stream::Static(Value::Number(3.0));
    assert_eq!(number(&stream, -1.0), 3.0);
    assert_eq!(number(&stream, 10.0), 3.0);
}

#[test]
fn linear_keys_interpolate_and_hold_past_the_ends() {
    let stream = Stream::Animated(vec![
        key(1.0, Value::Number(0.0), 0.0, None),
        key(2.0, Value::Number(10.0), 0.0, None),
    ]);
    assert_eq!(number(&stream, 0.0), 0.0);
    assert!((number(&stream, 1.25) - 2.5).abs() < 1e-12);
    assert_eq!(number(&stream, 3.0), 10.0);
}

#[test]
fn a_hold_keeps_its_value_until_the_next_key() {
    let mut first = key(0.0, Value::Number(1.0), 0.0, None);
    first.out_interpolation = Interpolation::Hold;
    let stream = Stream::Animated(vec![first, key(1.0, Value::Number(5.0), 0.0, None)]);
    assert_eq!(number(&stream, 0.99), 1.0);
    assert_eq!(number(&stream, 1.0), 5.0);
}

/// Speeds with a third's influence are the slopes of a cubic Hermite in
/// time, the curve the fitter writes.
#[test]
fn speeds_at_a_third_are_a_hermite_curve() {
    let (m0, m1) = (30.0, -12.0);
    let mut a = key(0.0, Value::Number(0.0), m0, Some(1.0 / 3.0));
    let b = key(2.0, Value::Number(10.0), m1, Some(1.0 / 3.0));
    a.out_ease[0].speed = m0;
    let stream = Stream::Animated(vec![a, b]);
    for i in 0..=20 {
        let t = f64::from(i) / 10.0;
        let u = t / 2.0;
        let (u2, u3) = (u * u, u * u * u);
        let hermite =
            (u3 - 2.0 * u2 + u) * 2.0 * m0 + (-2.0 * u3 + 3.0 * u2) * 10.0 + (u3 - u2) * 2.0 * m1;
        assert!((number(&stream, t) - hermite).abs() < 1e-9, "at {t}");
    }
}

#[test]
fn eases_are_per_dimension_where_the_project_keeps_one_per_dimension() {
    let mut a = key(0.0, Value::Vector2([0.0, 0.0]), 0.0, Some(1.0 / 3.0));
    a.out_ease.push(Ease {
        speed: 30.0,
        influence: 1.0 / 3.0,
    });
    let b = key(1.0, Value::Vector2([10.0, 10.0]), 0.0, Some(1.0 / 3.0));
    let Value::Vector2([x, y]) = Stream::Animated(vec![a, b]).value_at(0.1) else {
        panic!("a vector");
    };
    assert!(y > x, "the second dimension leaves faster: {x} {y}");
}

#[test]
fn a_spatial_key_moves_along_its_path_at_an_even_speed() {
    let spatial = |out: [f64; 2], into: [f64; 2]| Spatial {
        in_tangent: into.to_vec(),
        out_tangent: out.to_vec(),
        continuous: false,
        auto_bezier: false,
    };
    let mut a = key(0.0, Value::Vector3([0.0, 0.0, 0.0]), 0.0, None);
    let mut b = key(1.0, Value::Vector3([100.0, 0.0, 10.0]), 0.0, None);
    a.spatial = Some(spatial([0.0, 50.0], [0.0, 0.0]));
    b.spatial = Some(spatial([0.0, 0.0], [0.0, 50.0]));
    let stream = Stream::Animated(vec![a, b]);
    assert!(stream.is_spatial());
    let Value::Vector3([x, y, z]) = stream.value_at(0.5) else {
        panic!("a vector");
    };
    // The bend is symmetric in space too, so halfway along it is the
    // middle, raised.
    assert!((x - 50.0).abs() < 1e-6, "{x}");
    assert!((y - 37.5).abs() < 1e-6, "{y}");
    assert!((z - 5.0).abs() < 1e-6, "{z}");
}

/// Spatial handles leaving and arriving, in px with depth.
fn handles(out: [f64; 3], into: [f64; 3]) -> Spatial {
    Spatial {
        in_tangent: into.to_vec(),
        out_tangent: out.to_vec(),
        continuous: false,
        auto_bezier: false,
    }
}

/// Two linear keys a second apart from `from` to `to`, bent by `out` and
/// `into`.
fn span(from: [f64; 3], to: [f64; 3], out: [f64; 3], into: [f64; 3]) -> (Keyframe, Keyframe) {
    let mut a = key(0.0, Value::Vector3(from), 0.0, None);
    let mut b = key(1.0, Value::Vector3(to), 0.0, None);
    a.spatial = Some(handles(out, [0.0; 3]));
    b.spatial = Some(handles([0.0; 3], into));
    (a, b)
}

#[test]
fn a_span_moves_in_depth_level_straight_or_bent() {
    let origin = [0.0; 3];
    let cases = [
        // One depth, bent in the screen's plane: a flat path.
        (
            (
                origin,
                [100.0, 0.0, 0.0],
                [0.0, 50.0, 0.0],
                [0.0, 50.0, 0.0],
            ),
            Depth::Level,
        ),
        // No handles: a line in space.
        (
            (origin, [300.0, 0.0, 400.0], origin, origin),
            Depth::Straight,
        ),
        // Handles on the line, as automatic ones lie on a move in a line.
        (
            (
                origin,
                [300.0, 0.0, 400.0],
                [30.0, 0.0, 40.0],
                [-60.0, 0.0, -80.0],
            ),
            Depth::Straight,
        ),
        // A handle past the far end turns the path back over itself.
        (
            (origin, [300.0, 0.0, 400.0], [600.0, 0.0, 800.0], origin),
            Depth::Bent,
        ),
        // A handle off the line bends it.
        (
            (origin, [300.0, 0.0, 400.0], [0.0, 10.0, 0.0], origin),
            Depth::Bent,
        ),
        // A depth handle bends a path whose ends share their depth.
        (
            (origin, [100.0, 0.0, 0.0], [0.0, 0.0, 30.0], origin),
            Depth::Bent,
        ),
    ];
    for ((from, to, out, into), expected) in cases {
        let (a, b) = span(from, to, out, into);
        assert_eq!(
            depth(&a, &b),
            expected,
            "{from:?} -> {to:?}, {out:?} {into:?}"
        );
    }
}

#[test]
fn a_path_in_space_is_as_long_as_its_curve() {
    // A curve in the plane of X and depth, measured as a flat curve there.
    let (from, to, out, into) = (
        [0.0, 7.0, 0.0],
        [100.0, 7.0, 100.0],
        [55.0, 0.0, 0.0],
        [0.0, 0.0, -55.0],
    );
    let path = Path3::new(from, to, [out, into]);
    let flat = kurbo::CubicBez::new((0.0, 0.0), (55.0, 0.0), (100.0, 45.0), (100.0, 100.0));
    let expected = kurbo::ParamCurveArclen::arclen(&flat, 1e-9);
    assert!(
        (path.length - expected).abs() < 1e-6,
        "{} vs {expected}",
        path.length
    );
    // A point along it is that far along the curve.
    let along = path.at_length(expected / 3.0);
    let t = kurbo::ParamCurveArclen::inv_arclen(&flat, expected / 3.0, 1e-9);
    let flat_point = kurbo::ParamCurve::eval(&flat, t);
    let gap = [
        along[0] - flat_point.x,
        along[1] - 7.0,
        along[2] - flat_point.y,
    ];
    assert!(
        gap.iter().all(|g| g.abs() < 1e-4),
        "{along:?} vs {flat_point:?}"
    );
    // Past the end it carries on along the last handle.
    let past = path.at_length(expected + 10.0);
    assert!(
        (past[2] - 110.0).abs() < 1e-9 && (past[0] - 100.0).abs() < 1e-9,
        "{past:?}"
    );
}

#[test]
fn a_point_with_depth_is_timed_along_its_length_in_space() {
    // A straight move of 500 px in space, eased: halfway in time is
    // halfway along the line.
    let mut a = key(0.0, Value::Vector3([0.0, 0.0, 0.0]), 0.0, Some(0.5));
    let mut b = key(1.0, Value::Vector3([300.0, 0.0, 400.0]), 0.0, Some(0.5));
    a.spatial = Some(handles([0.0; 3], [0.0; 3]));
    b.spatial = a.spatial.clone();
    let stream = Stream::Animated(vec![a, b]);
    let Value::Vector3([x, y, z]) = stream.value_at(0.5) else {
        panic!("a vector");
    };
    assert!((x - 150.0).abs() < 1e-6 && y.abs() < 1e-9 && (z - 200.0).abs() < 1e-6);
    // A bent one sits on its curve, at the time its length gives.
    let (a, b) = span(
        [0.0; 3],
        [100.0, 0.0, 100.0],
        [55.0, 0.0, 0.0],
        [0.0, 0.0, -55.0],
    );
    let path = Path3::new(
        [0.0; 3],
        [100.0, 0.0, 100.0],
        [[55.0, 0.0, 0.0], [0.0, 0.0, -55.0]],
    );
    let stream = Stream::Animated(vec![a, b]);
    let Value::Vector3(got) = stream.value_at(0.25) else {
        panic!("a vector");
    };
    let expected = path.at_length(path.length / 4.0);
    assert!(
        got.iter().zip(expected).all(|(g, e)| (g - e).abs() < 1e-9),
        "{got:?} vs {expected:?}"
    );
}

#[test]
fn numbers_round_trip_through_a_value_of_each_kind() {
    let colour = Value::Color(Color {
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 1.0,
    });
    assert_eq!(colour.numbers(), Some(vec![0.1, 0.2, 0.3, 1.0]));
    assert_eq!(
        colour.with_numbers(&[1.0, 0.5]).numbers(),
        Some(vec![1.0, 0.5, 0.3, 1.0])
    );
    assert_eq!(
        Value::Vector3([1.0, 2.0, 3.0]).with_numbers(&[9.0]),
        Value::Vector3([9.0, 2.0, 3.0])
    );
}
