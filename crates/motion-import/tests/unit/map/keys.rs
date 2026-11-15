//! Unit tests for `src/map/keys.rs`: values, paths, clocks and curves.

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;
use crate::model::Interpolation;
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

#[test]
fn clamping_a_track_says_whether_the_motion_between_keys_changes() {
    let held = |curve: &str| Written::Track {
        keys: json!([
            {"t": "0s", "v": 0.5, "curve": curve},
            {"t": "1s", "v": 5.5, "curve": "hold"},
            {"t": "2s", "v": [0.2, -3]},
        ]),
        first: json!(0.5),
    };
    let mut holds = held("hold");
    assert_eq!(
        holds.clamp((0.0, 1.0)),
        Clamped {
            count: 2,
            between_keys: false
        }
    );
    let Written::Track { keys, .. } = &holds else {
        unreachable!()
    };
    assert_eq!(keys[1]["v"], json!(1));
    assert_eq!(keys[2]["v"], json!([0.2, 0]));
    let mut eased = held("ease");
    assert!(
        eased.clamp((0.0, 1.0)).between_keys,
        "the key after an ease"
    );
    let mut still = Written::Value(json!(-179));
    assert_eq!(still.clamp((0.0, f64::INFINITY)).count, 1);
    assert!(matches!(still, Written::Value(v) if v == json!(0)));
    let mut inside = Written::Value(json!([0.1, 0.2]));
    assert_eq!(inside.clamp((0.0, 1.0)), Clamped::default());
}

/// The point and depth parts a stream of 3D vectors is written as.
fn parts(stream: &Stream, spatial: bool, fps: f64) -> (serde_json::Value, serde_json::Value) {
    let json = |w: Written| match w {
        Written::Value(v) => v,
        Written::Track { keys, .. } => keys,
    };
    let (point, depth) = split(stream, Convert::Point, spatial, SAME, fps).expect("it splits");
    (json(point), json(depth.expect("a depth")))
}

/// A linear key at `time` at `[x, y, z]`, with spatial handles `out` and
/// `into` in space.
fn spatial_key(time: f64, value: [f64; 3], out: [f64; 3], into: [f64; 3]) -> Keyframe {
    Keyframe {
        spatial: Some(Spatial {
            in_tangent: into.to_vec(),
            out_tangent: out.to_vec(),
            continuous: false,
            auto_bezier: false,
        }),
        ..key(time, Value::Vector3(value), Interpolation::Linear, 0.0, 0.0)
    }
}

#[test]
fn a_3d_vector_splits_into_a_point_and_its_depth() {
    let still = Stream::Static(Value::Vector3([10.0, 20.0, -300.0]));
    assert_eq!(parts(&still, true, 10.0), (json!([10, 20]), json!(-300)));
    // Scale keeps an ease per dimension: the depth takes the third.
    let mut a = key(
        0.0,
        Value::Vector3([1.0, 1.0, 1.0]),
        Interpolation::Bezier,
        0.0,
        0.5,
    );
    let b = key(
        1.0,
        Value::Vector3([2.0, 2.0, 3.0]),
        Interpolation::Bezier,
        0.0,
        0.5,
    );
    a.out_ease = vec![
        Ease {
            speed: 1.0,
            influence: 0.5,
        },
        Ease {
            speed: 1.0,
            influence: 0.5,
        },
        Ease {
            speed: 4.0,
            influence: 0.25,
        },
    ];
    let (point, depth) = parts(&Stream::Animated(vec![a, b]), false, 10.0);
    assert_eq!(point[0]["curve"], "speed(1,0.5,0,0.5)");
    assert_eq!(depth[0]["v"], 1);
    assert_eq!(depth[0]["curve"], "speed(4,0.25,0,0.5)");
    // A flat vector has no depth to write.
    let flat = Stream::Static(Value::Vector2([1.0, 2.0]));
    let (_, depth) = split(&flat, Convert::Point, false, SAME, 10.0).expect("it splits");
    assert!(depth.is_none());
}

#[test]
fn a_path_at_one_depth_keeps_its_flat_bend() {
    let keys = vec![
        spatial_key(0.0, [0.0, 0.0, 50.0], [0.0, 40.0, 0.0], [0.0; 3]),
        spatial_key(1.0, [100.0, 0.0, 50.0], [0.0; 3], [0.0, 40.0, 0.0]),
    ];
    let (point, depth) = parts(&Stream::Animated(keys), true, 10.0);
    assert_eq!(point[0]["path"], json!([[0, 40], [0, 40]]));
    assert_eq!(point.as_array().map(Vec::len), Some(2));
    assert_eq!(depth, json!(50), "a depth that never moves is one value");
}

#[test]
fn a_straight_move_in_space_shares_its_speed_between_point_and_depth() {
    // 500 px in space, of which 300 across and 400 in depth, leaving at
    // 1000 px a second.
    let mut a = spatial_key(0.0, [0.0, 0.0, 0.0], [0.0; 3], [0.0; 3]);
    let mut b = spatial_key(1.0, [300.0, 0.0, 400.0], [0.0; 3], [0.0; 3]);
    for k in [&mut a, &mut b] {
        k.in_interpolation = Interpolation::Bezier;
        k.out_interpolation = Interpolation::Bezier;
    }
    a.out_ease = vec![Ease {
        speed: 1000.0,
        influence: 0.25,
    }];
    b.in_ease = vec![Ease {
        speed: 0.0,
        influence: 0.5,
    }];
    let (point, depth) = parts(&Stream::Animated(vec![a, b]), true, 10.0);
    assert_eq!(point[0]["curve"], "speed(600,0.25,0,0.5)");
    assert!(point[0].get("path").is_none(), "a straight line");
    assert_eq!(depth[0]["curve"], "speed(800,0.25,0,0.5)");
    assert_eq!(depth[1]["v"], 400);
}

#[test]
fn a_path_that_bends_in_depth_is_keyed_at_every_frame() {
    let keys = vec![
        spatial_key(0.0, [0.0, 0.0, 0.0], [55.0, 0.0, 0.0], [0.0; 3]),
        spatial_key(1.0, [100.0, 0.0, 100.0], [0.0; 3], [0.0, 0.0, -55.0]),
    ];
    let stream = Stream::Animated(keys);
    let (point, depth) = parts(&stream, true, 4.0);
    let times: Vec<&str> = point
        .as_array()
        .expect("keys")
        .iter()
        .filter_map(|k| k["t"].as_str())
        .collect();
    assert_eq!(times, ["0s", "0.25s", "0.5s", "0.75s", "1s"]);
    let Value::Vector3([x, _, z]) = stream.value_at(0.25) else {
        panic!("a vector");
    };
    assert_eq!(point[1]["v"], json!([num(x), 0]));
    assert_eq!(depth[1]["v"], num(z));
    assert_eq!(point[1]["curve"], "linear");
    assert!(point[0].get("path").is_none());
}
