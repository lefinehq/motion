//! Unit tests for `src/map/masks.rs`: masks from a layer's mask parade.

use pretty_assertions::assert_eq;
use serde_json::json;

use crate::map::{Options, import};
use crate::model::{Child, MaskSettings, Path, Stream, Value, Vertex};
use crate::test_support::build::{comp, group, layer, project, property};

/// A mask group with `settings`, holding `properties`.
fn mask(settings: MaskSettings, properties: Vec<crate::model::Property>) -> Child {
    Child::Group(crate::model::Group {
        mask: Some(settings),
        ..group(
            "ADBE Mask Atom",
            properties.into_iter().map(Child::Property).collect(),
        )
    })
}

/// Settings of `mode`, inverted or not, with nothing else set.
fn settings(mode: u32, inverted: bool) -> MaskSettings {
    MaskSettings {
        inverted,
        motion_blur: 0,
        linear_falloff: false,
        mode,
        id: 1,
    }
}

/// The scene's masks and the report's unsupported findings for a layer
/// without a source whose mask parade holds `masks`.
fn imported(masks: Vec<Child>) -> (serde_json::Value, Vec<String>) {
    let title = layer(
        1,
        "Title",
        vec![Child::Group(group("ADBE Mask Parade", masks))],
    );
    let import = import(
        &project(vec![(5, "Main", comp(vec![title], 2.0))]),
        &Options::default(),
    )
    .expect("it imports");
    let unsupported = import
        .mapping
        .unsupported
        .iter()
        .map(|f| f.what.clone())
        .collect();
    (import.json["layers"][0]["mask"].clone(), unsupported)
}

#[test]
fn a_mask_without_a_path_covers_its_whole_space_and_settings_follow_mkif() {
    let triangle = Path {
        closed: true,
        vertices: [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]
            .map(|point| Vertex {
                point,
                in_tangent: [0.0, 0.0],
                out_tangent: [0.0, 0.0],
            })
            .to_vec(),
    };
    let (masks, unsupported) = imported(vec![
        mask(settings(1, false), Vec::new()),
        mask(
            settings(2, true),
            vec![
                property("ADBE Mask Shape", Stream::Static(Value::Path(triangle))),
                property("ADBE Mask Opacity", Stream::Static(Value::Number(50.0))),
                property("ADBE Mask Offset", Stream::Static(Value::Number(-3.0))),
            ],
        ),
    ]);
    assert_eq!(
        masks,
        json!([
            {"mask": "M0,0 C0,0 0,1 0,1 C0,1 1,1 1,1 C1,1 1,0 1,0 C1,0 0,0 0,0 Z"},
            {"mask": "M0,0 C0,0 10,0 10,0 C10,0 0,10 0,10 C0,10 0,0 0,0 Z",
             "opacity": 0.5, "expand": -3, "mode": "subtract", "keep": "outside"}
        ])
    );
    assert!(unsupported.is_empty(), "{unsupported:?}");
}

#[test]
fn what_the_scene_cannot_draw_of_a_mask_is_reported() {
    let blurred = MaskSettings {
        motion_blur: 2,
        linear_falloff: true,
        ..settings(9, false)
    };
    let (_, unsupported) = imported(vec![mask(
        blurred,
        vec![property(
            "ADBE Mask Feather",
            Stream::Static(Value::Vector2([4.0, 4.0])),
        )],
    )]);
    assert_eq!(
        unsupported,
        [
            "mask mode 9",
            "mask motion blur",
            "a mask feather that falls off linearly"
        ]
    );
}

#[test]
fn keyed_mask_paths_take_the_vertex_count_of_the_largest() {
    use crate::model::{Interpolation, Keyframe};
    let path = |points: &[[f64; 2]]| {
        Value::Path(Path {
            closed: true,
            vertices: points
                .iter()
                .map(|&point| Vertex {
                    point,
                    in_tangent: [0.0, 0.0],
                    out_tangent: [0.0, 0.0],
                })
                .collect(),
        })
    };
    let key = |time, value| Keyframe {
        time,
        value,
        in_interpolation: Interpolation::Hold,
        out_interpolation: Interpolation::Hold,
        in_ease: Vec::new(),
        out_ease: Vec::new(),
        spatial: None,
        continuous: false,
        auto_bezier: false,
        roving: false,
    };
    let shape = property(
        "ADBE Mask Shape",
        Stream::Animated(vec![
            key(0.0, path(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]])),
            key(1.0, path(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])),
        ]),
    );
    let (masks, _) = imported(vec![mask(settings(1, false), vec![shape])]);
    let keys = masks[0]["anim"]["path"].as_array().expect("path keys");
    let commands = |k: &serde_json::Value| k["v"].as_str().unwrap().matches('C').count();
    assert_eq!(keys.iter().map(commands).collect::<Vec<_>>(), [4, 4]);
}
