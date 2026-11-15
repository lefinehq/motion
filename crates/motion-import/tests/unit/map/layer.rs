//! Unit tests for `src/map/layer.rs`: 3D layers, cameras and what they
//! report.

use pretty_assertions::assert_eq;
use serde_json::{Value as Json, json};

use crate::map::{Import, Options, import};
use crate::model::{
    AutoOrient, Child, Composition, Ease, Interpolation, Keyframe, Layer, LayerKind, Renderer,
    Spatial, Stream, Value,
};
use crate::test_support::build::{comp, group, layer, number, project, property, transform};

/// `layers` imported as the scene of a 200 by 100 composition at 10 fps.
fn imported_comp(main: Composition) -> Import {
    import(&project(vec![(1, "Main", main)]), &Options::default()).expect("it imports")
}

/// `layers` imported as the scene of a 200 by 100 composition.
fn imported(layers: Vec<Layer>) -> Import {
    imported_comp(comp(layers, 2.0))
}

/// What the import reported as unsupported.
fn reported(import: &Import) -> Vec<&str> {
    import
        .mapping
        .unsupported
        .iter()
        .map(|finding| finding.what.as_str())
        .collect()
}

/// A 3D vector all the time.
fn vector(x: f64, y: f64, z: f64) -> Stream {
    Stream::Static(Value::Vector3([x, y, z]))
}

/// A layer with the 3D switch `on`, whose transform holds `properties`.
fn spatial_layer(on: bool, properties: Vec<crate::model::Property>) -> Layer {
    let mut card = layer(1, "Card", vec![transform(properties)]);
    card.switches.three_d = on;
    card
}

/// The transform a 3D layer and a flat one take from the same properties.
fn turned() -> Vec<crate::model::Property> {
    vec![
        property("ADBE Anchor Point", vector(10.0, 20.0, 5.0)),
        property("ADBE Position", vector(100.0, 50.0, -300.0)),
        property("ADBE Scale", vector(1.0, 1.0, 2.0)),
        property("ADBE Orientation", vector(0.0, 90.0, 0.0)),
        property("ADBE Rotate X", number(30.0)),
        property("ADBE Rotate Y", number(-45.0)),
    ]
}

/// The keys of a layer that are not its id, in the order written.
fn transform_keys(layer: &Json) -> Json {
    let mut keys = layer.as_object().expect("a layer").clone();
    keys.remove("id");
    keys.remove("pivot");
    Json::Object(keys)
}

#[test]
fn a_3d_layer_keeps_its_depth_and_turns() {
    let import = imported(vec![spatial_layer(true, turned())]);
    assert_eq!(
        transform_keys(&import.json["layers"][0]),
        json!({
            "anchor": [10, 20], "anchor-z": 5,
            "position": [100, 50], "z": -300,
            "scale": [1, 1], "scale-z": 2,
            "orient": [0, 90, 0], "rotate-x": 30, "rotate-y": -45,
            "3d": true,
        })
    );
    assert!(reported(&import).is_empty(), "{:?}", reported(&import));
}

#[test]
fn a_flat_layer_drops_what_only_a_3d_layer_takes() {
    let import = imported(vec![spatial_layer(false, turned())]);
    assert_eq!(
        transform_keys(&import.json["layers"][0]),
        json!({"anchor": [10, 20], "position": [100, 50], "scale": [1, 1]})
    );
    assert!(reported(&import).is_empty(), "{:?}", reported(&import));
}

#[test]
fn a_separated_position_keeps_its_depth_on_a_3d_layer() {
    // A separated position is stored whole too, hidden.
    let whole = crate::model::Property {
        hidden: true,
        separated: true,
        ..property("ADBE Position", vector(10.0, 20.0, -40.0))
    };
    let parts = vec![
        whole,
        property("ADBE Position_0", number(10.0)),
        property("ADBE Position_1", number(20.0)),
        property("ADBE Position_2", number(-40.0)),
    ];
    let import = imported(vec![spatial_layer(true, parts.clone())]);
    let layer = &import.json["layers"][0];
    assert_eq!(
        (&layer["position"], &layer["z"]),
        (&json!([10, 20]), &json!(-40))
    );
    let flat = imported(vec![spatial_layer(false, parts)]);
    assert!(flat.json["layers"][0].get("z").is_none());
}

/// A linear key at `time` at `value`.
fn linear(time: f64, value: Value) -> Keyframe {
    Keyframe {
        time,
        value,
        in_interpolation: Interpolation::Linear,
        out_interpolation: Interpolation::Linear,
        in_ease: vec![Ease {
            speed: 0.0,
            influence: 0.0,
        }],
        out_ease: vec![Ease {
            speed: 0.0,
            influence: 0.0,
        }],
        spatial: None,
        continuous: false,
        auto_bezier: false,
        roving: false,
    }
}

#[test]
fn a_position_that_bends_in_depth_is_keyed_at_every_frame() {
    let bend = |out: [f64; 3], into: [f64; 3]| {
        Some(Spatial {
            in_tangent: into.to_vec(),
            out_tangent: out.to_vec(),
            continuous: false,
            auto_bezier: false,
        })
    };
    let mut from = linear(0.0, Value::Vector3([0.0, 0.0, 0.0]));
    let mut to = linear(1.0, Value::Vector3([100.0, 0.0, 100.0]));
    from.spatial = bend([55.0, 0.0, 0.0], [0.0; 3]);
    to.spatial = bend([0.0; 3], [0.0, 0.0, -55.0]);
    let keys = Stream::Animated(vec![from, to]);
    let import = imported(vec![spatial_layer(
        true,
        vec![property("ADBE Position", keys.clone())],
    )]);
    let anim = &import.json["layers"][0]["anim"];
    // At 10 fps over a second: the two keys and the nine frames between.
    assert_eq!(anim["position"].as_array().map(Vec::len), Some(11));
    assert_eq!(anim["z"].as_array().map(Vec::len), Some(11));
    let Value::Vector3([x, y, z]) = keys.value_at(0.5) else {
        panic!("a vector");
    };
    let close = |json: &Json, n: f64| (json.as_f64().expect("a number") - n).abs() < 1e-5;
    assert!(close(&anim["position"][5]["v"][0], x) && close(&anim["position"][5]["v"][1], y));
    assert!(close(&anim["z"][5]["v"], z));
}

#[test]
fn orientation_keys_turn_by_one_curve() {
    let keys = Stream::Animated(vec![
        linear(0.0, Value::Vector3([0.0, 0.0, 0.0])),
        linear(1.0, Value::Vector3([90.0, 0.0, 45.0])),
    ]);
    let import = imported(vec![spatial_layer(
        true,
        vec![property("ADBE Orientation", keys)],
    )]);
    assert_eq!(
        import.json["layers"][0]["anim"]["orient"],
        json!([
            {"t": "0s", "v": [0, 0, 0], "curve": "linear"},
            {"t": "1s", "v": [90, 0, 45]},
        ])
    );
}

/// A camera turning towards its point of interest or not, whose transform
/// holds `properties` and whose options hold `options`.
fn camera(
    orient: AutoOrient,
    properties: Vec<crate::model::Property>,
    options: Vec<crate::model::Property>,
) -> Layer {
    let options = Child::Group(group(
        "ADBE Camera Options Group",
        options.into_iter().map(Child::Property).collect(),
    ));
    let mut camera = layer(1, "Camera", vec![transform(properties), options]);
    camera.kind = LayerKind::Camera;
    camera.switches.three_d = true;
    camera.switches.orient = orient;
    camera
}

#[test]
fn a_two_node_camera_looks_at_its_point_of_interest() {
    let import = imported(vec![camera(
        AutoOrient::TowardsPoint,
        vec![
            property("ADBE Anchor Point", vector(100.0, 50.0, 10.0)),
            property("ADBE Position", vector(100.0, 50.0, -500.0)),
            property("ADBE Scale", vector(1.0, 1.0, 1.0)),
            property("ADBE Opacity", number(1.0)),
        ],
        vec![property("ADBE Camera Zoom", number(500.0))],
    )]);
    assert_eq!(
        transform_keys(&import.json["layers"][0]),
        json!({
            "camera": true,
            "target": [100, 50], "target-z": 10,
            "position": [100, 50], "z": -500,
            "zoom": 500,
        })
    );
    assert!(reported(&import).is_empty(), "{:?}", reported(&import));
}

#[test]
fn a_camera_left_at_its_defaults_sees_the_plane_at_depth_0_at_full_size() {
    // A 50 mm lens on 36 mm film over the composition's 200 px.
    let zoom = 200.0 * 50.0 / 36.0;
    let one_node = imported(vec![camera(
        AutoOrient::Off,
        vec![property("ADBE Anchor Point", vector(5.0, 5.0, 0.0))],
        vec![],
    )]);
    let layer = &one_node.json["layers"][0];
    assert_eq!(
        (&layer["position"], &layer["z"], &layer["zoom"]),
        (
            &json!([100, 50]),
            &json!(crate::map::keys::num(-zoom)),
            &json!(crate::map::keys::num(zoom))
        )
    );
    assert!(
        layer.get("target").is_none(),
        "a one-node camera aims nowhere"
    );
}

#[test]
fn depth_of_field_is_reported_where_it_is_on() {
    let options = |on: f64| {
        vec![
            property("ADBE Camera Depth of Field", number(on)),
            property("ADBE Camera Focus Distance", number(900.0)),
            property("ADBE Camera Aperture", number(25.3)),
        ]
    };
    let on = imported(vec![camera(AutoOrient::TowardsPoint, vec![], options(1.0))]);
    assert_eq!(reported(&on), ["depth of field (drawn sharp)"]);
    let off = imported(vec![camera(AutoOrient::TowardsPoint, vec![], options(0.0))]);
    assert!(reported(&off).is_empty(), "{:?}", reported(&off));
}

#[test]
fn a_light_stays_reported_and_turning_by_itself_is_reported() {
    let mut light = layer(1, "Light", vec![]);
    light.kind = LayerKind::Light;
    light.switches.three_d = true;
    light.switches.orient = AutoOrient::TowardsPoint;
    let mut along = layer(2, "Arrow", vec![]);
    along.switches.orient = AutoOrient::AlongPath;
    let mut facing = spatial_layer(true, vec![]);
    facing.id = 3;
    facing.switches.orient = AutoOrient::TowardsCamera;
    let import = imported(vec![light, along, facing]);
    let mut found = reported(&import);
    found.sort_unstable();
    assert_eq!(
        found,
        [
            "lights (drawn as nothing)",
            "turning along the motion path (drawn as it is turned)",
            "turning to face the camera (drawn as it is turned)",
        ]
    );
}

#[test]
fn a_renderer_other_than_the_classic_one_is_reported_where_it_draws_3d_layers() {
    let renderer = Some(Renderer {
        match_name: "ADBE Calder".into(),
        name: "Advanced 3D".into(),
    });
    let with = |three_d: bool| {
        imported_comp(Composition {
            renderer: renderer.clone(),
            ..comp(vec![spatial_layer(three_d, vec![])], 2.0)
        })
    };
    assert_eq!(
        reported(&with(true)),
        ["the `Advanced 3D` renderer (drawn as the classic renderer draws)"]
    );
    assert!(reported(&with(false)).is_empty());
}
