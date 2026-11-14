//! Unit tests for `src/map/keys.rs`: values, paths, clocks and curves.

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;
use crate::model::{Ease, Spatial, Vertex};

/// A clock at the composition's own time.
const SAME: Clock = Clock {
    start: 0.0,
    stretch: 1.0,
};

/// A keyframe at `time` holding `value`, with `interpolation` on both sides
/// and one ease per side of `speed` and `influence`.
fn key(
    time: f64,
    value: Value,
    interpolation: Interpolation,
    speed: f64,
    influence: f64,
) -> Keyframe {
    let ease = vec![Ease { speed, influence }];
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

/// The key list of `keys` written through `convert`.
fn track(keys: Vec<Keyframe>, convert: Convert, spatial: bool, clock: Clock) -> serde_json::Value {
    match written(&Stream::Animated(keys), convert, spatial, clock) {
        Some(Written::Track { keys, .. }) => keys,
        other => panic!("not a track: {other:?}"),
    }
}

#[test]
fn values_convert_to_the_scenes_units() {
    let cases = [
        (Convert::Number, Value::Number(45.0), json!(45)),
        (Convert::Percent, Value::Number(64.0), json!(0.64)),
        (
            Convert::Point,
            Value::Vector3([511.4999999999902, 455.0, 0.0]),
            json!([511.5, 455]),
        ),
        (
            Convert::PointPercent,
            Value::Vector2([100.0, 50.0]),
            json!([1, 0.5]),
        ),
        (
            Convert::Color,
            Value::Color(Color {
                r: 1.0,
                g: 0.8825368285179138,
                b: 0.0,
                a: 1.0,
            }),
            json!([1, 0.882537, 0, 1]),
        ),
    ];
    for (convert, value, expected) in cases {
        assert_eq!(convert.value(&value), Some(expected), "{convert:?}");
    }
    assert_eq!(Convert::Point.value(&Value::Number(1.0)), None);
    assert_eq!(Convert::Color.value(&Value::Vector2([1.0, 2.0])), None);
}

#[test]
fn paths_become_one_cubic_per_segment() {
    let vertex = |point, in_tangent, out_tangent| Vertex {
        point,
        in_tangent,
        out_tangent,
    };
    let path = Path {
        closed: true,
        vertices: vec![
            vertex([0.0, 0.0], [0.0, 0.0], [10.0, 0.0]),
            vertex([20.0, 20.0], [0.0, -10.0], [0.0, 0.0]),
        ],
    };
    assert_eq!(svg(&path), "M0,0 C10,0 20,10 20,20 C20,20 0,0 0,0 Z");
    let open = Path {
        closed: false,
        ..path
    };
    assert_eq!(svg(&open), "M0,0 C10,0 20,10 20,20");
    assert_eq!(
        svg(&Path {
            closed: false,
            vertices: vec![]
        }),
        ""
    );
}

#[test]
fn key_times_are_in_the_layers_own_seconds() {
    let late = Clock {
        start: -2.52,
        stretch: 1.0,
    };
    assert_eq!(late.time(0.48), "3s");
    let slow = Clock {
        start: 1.0,
        stretch: 2.0,
    };
    assert_eq!(slow.time(5.0), "2s");
    let reversed = Clock {
        start: 4.0,
        stretch: -1.0,
    };
    assert!((reversed.local(4.0 - 1.0 / 3000.0) - 0.0).abs() < 1e-9);
}

/// The curve of each key in a written key list; `Null` for the last.
fn curves(written: &serde_json::Value) -> Vec<serde_json::Value> {
    written
        .as_array()
        .expect("a key list")
        .iter()
        .map(|k| k["curve"].clone())
        .collect()
}

#[test]
fn curves_keep_holds_linear_spans_and_eases() {
    let n = Value::Number;
    let keys = vec![
        key(0.0, n(0.0), Interpolation::Linear, 0.0, 0.0),
        key(1.0, n(10.0), Interpolation::Linear, 0.0, 0.0),
        key(2.0, n(20.0), Interpolation::Bezier, 5.0, 0.5022963880959406),
        key(3.0, n(0.0), Interpolation::Hold, 0.0, 0.0),
        key(4.0, n(5.0), Interpolation::Linear, 0.0, 0.0),
    ];
    assert_eq!(
        curves(&track(keys, Convert::Number, false, SAME)),
        [
            json!("linear"),
            json!("speed(0,0,5,0.502296)"),
            json!("speed(5,0.502296,0,0)"),
            json!("hold"),
            serde_json::Value::Null,
        ]
    );
}

#[test]
fn speeds_scale_with_the_units_and_the_stretch() {
    let keys = vec![
        key(0.0, Value::Number(0.0), Interpolation::Bezier, 50.0, 0.25),
        key(1.0, Value::Number(100.0), Interpolation::Bezier, 50.0, 0.25),
    ];
    let slow = Clock {
        start: 0.0,
        stretch: 2.0,
    };
    let written = track(keys, Convert::Percent, false, slow);
    assert_eq!(curves(&written)[0], json!("speed(1,0.25,1,0.25)"));
    assert_eq!(written[1]["v"], json!(1));
    assert_eq!(written[1]["t"], json!("0.5s"));
}

#[test]
fn eases_per_dimension_become_curves_per_component() {
    let mut first = key(
        0.0,
        Value::Vector3([0.0, 0.0, 0.0]),
        Interpolation::Bezier,
        0.0,
        0.5,
    );
    first.out_ease = vec![
        Ease {
            speed: 0.0,
            influence: 0.5,
        },
        Ease {
            speed: 0.0,
            influence: 0.9,
        },
        Ease {
            speed: 0.0,
            influence: 0.1,
        },
    ];
    let second = key(
        1.0,
        Value::Vector3([0.7, 0.7, 0.9]),
        Interpolation::Bezier,
        0.0,
        0.5,
    );
    let written = track(vec![first, second], Convert::Point, false, SAME);
    assert_eq!(
        curves(&written)[0],
        json!(["speed(0,0.5,0,0.5)", "speed(0,0.9,0,0.5)"])
    );
}

#[test]
fn spatial_keys_take_one_curve_and_keep_their_bends() {
    let mut first = key(
        0.0,
        Value::Vector3([0.0, 0.0, 0.0]),
        Interpolation::Bezier,
        100.0,
        0.33,
    );
    first.spatial = Some(Spatial {
        in_tangent: vec![0.0, 0.0, 0.0],
        out_tangent: vec![10.0, -20.0, 0.0],
        continuous: false,
        auto_bezier: false,
    });
    let mut second = key(
        1.0,
        Value::Vector3([100.0, 0.0, 0.0]),
        Interpolation::Bezier,
        0.0,
        0.33,
    );
    second.spatial = Some(Spatial {
        in_tangent: vec![-10.0, 0.0, 0.0],
        out_tangent: vec![0.0, 0.0, 0.0],
        continuous: false,
        auto_bezier: false,
    });
    let straight = |mut k: Keyframe| {
        k.spatial = None;
        k
    };
    let written = track(
        vec![first.clone(), second.clone()],
        Convert::Point,
        true,
        SAME,
    );
    assert_eq!(written[0]["curve"], json!("speed(100,0.33,0,0.33)"));
    assert_eq!(written[0]["path"], json!([[10, -20], [-10, 0]]));
    let written = track(
        vec![straight(first), straight(second)],
        Convert::Point,
        true,
        SAME,
    );
    assert_eq!(written[0].get("path"), None);
}

#[test]
fn a_track_names_its_first_value_and_refuses_a_foreign_one() {
    let keys = vec![
        key(0.0, Value::Number(3.0), Interpolation::Linear, 0.0, 0.0),
        key(1.0, Value::Number(4.0), Interpolation::Linear, 0.0, 0.0),
    ];
    match written(
        &Stream::Animated(keys.clone()),
        Convert::Number,
        false,
        SAME,
    ) {
        Some(Written::Track { first, .. }) => assert_eq!(first, json!(3)),
        other => panic!("not a track: {other:?}"),
    }
    assert_eq!(
        written(&Stream::Animated(keys), Convert::Color, false, SAME),
        None
    );
    assert_eq!(
        written(
            &Stream::Static(Value::Number(2.5)),
            Convert::Number,
            false,
            SAME
        ),
        Some(Written::Value(json!(2.5)))
    );
}
