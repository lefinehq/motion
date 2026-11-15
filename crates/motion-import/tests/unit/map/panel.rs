//! Unit tests for `src/map/panel.rs`, on small projects with a panel.

use pretty_assertions::assert_eq;
use serde_json::{Value as Json, json};

use crate::map::{Import, Options, import};
use crate::model::{ControlKind, ControlTarget, PanelControl, PathStep};
use crate::template::Control;
use crate::test_support::build::{
    comp, expressed, layer, number, project, property, slider, transform,
};

const SPEED: &str = r#"thisComp.layer("CONTROLS").effect("Speed")("Slider")"#;

fn step(match_name: &str, index: Option<u32>) -> PathStep {
    PathStep {
        match_name: match_name.into(),
        index,
    }
}

/// A control of the template's panel.
fn offered(id: &str, kind: ControlKind, name: &str, value: Json) -> Control {
    Control {
        id: id.into(),
        kind,
        name: name.into(),
        value,
        min: None,
        max: None,
    }
}

/// The template's panel: a group `Motion` holding `Turn`, an angle that
/// drives `L1`'s rotation; `Pace`, the slider `L2` reads; and `Tint`, which
/// the project does not keep.
fn panel() -> Vec<Control> {
    vec![
        offered("g", ControlKind::Group, "Motion", json!(["rot"])),
        offered("spd", ControlKind::Slider, "Pace", json!(4)),
        offered("rot", ControlKind::Angle, "Turn", json!(30)),
        offered("tint", ControlKind::Color, "Tint", json!([1, 0, 0, 1])),
    ]
}

/// Main, item 5, with the panel as the project keeps it, and `Other`,
/// item 6, which no layer draws either.
fn imported(options: Options) -> Import {
    imported_reading(options, SPEED)
}

/// As [`imported`], with `L2`'s rotation under the expression `read`.
fn imported_reading(options: Options, read: &str) -> Import {
    let kept = |id: &str, kind, layer, path| PanelControl {
        id: id.into(),
        kind,
        targets: vec![ControlTarget {
            composition: 5,
            layer,
            path,
        }],
    };
    let mut main = comp(
        vec![
            layer(
                1,
                "L1",
                vec![transform(vec![property("ADBE Rotate Z", number(30.0))])],
            ),
            layer(
                2,
                "L2",
                vec![transform(vec![expressed(
                    "ADBE Rotate Z",
                    number(0.0),
                    read,
                )])],
            ),
            layer(
                9,
                "CONTROLS",
                vec![slider(
                    "Speed",
                    Some(property("ADBE Slider Control-0001", number(4.0))),
                )],
            ),
        ],
        2.0,
    );
    main.panel = vec![
        kept(
            "spd",
            ControlKind::Slider,
            9,
            vec![
                step("ADBE Effect Parade", None),
                step("ADBE Slider Control", Some(0)),
                step("ADBE Slider Control-0001", Some(1)),
            ],
        ),
        kept(
            "rot",
            ControlKind::Angle,
            1,
            vec![
                step("ADBE Transform Group", None),
                step("ADBE Rotate Z", None),
            ],
        ),
    ];
    let other = comp(vec![layer(1, "X", vec![])], 2.0);
    import(
        &project(vec![(5, "Main", main), (6, "Other", other)]),
        &options,
    )
    .expect("it imports")
}

fn rotation(import: &Import, id: &str) -> Json {
    import.json["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == json!(id))
        .expect("the layer")["rotate"]
        .clone()
}

#[test]
fn each_control_becomes_a_slot_in_the_panels_order() {
    let import = imported(Options {
        panel: panel(),
        ..Options::default()
    });
    assert_eq!(import.mapping.scene, "Main");
    assert_eq!(
        import.json["slots"],
        json!({
            "pace": {"number": 4, "label": "Pace"},
            "turn": {"number": 30, "label": "Motion / Turn"},
        })
    );
    assert_eq!(rotation(&import, "l1"), json!("$turn"));
    assert_eq!(rotation(&import, "l2"), json!("$pace"));
    let report = import.mapping.to_string();
    assert!(
        report.contains("  template: 3 controls, 2 slots\n"),
        "{report}"
    );
    assert!(
        report.contains("    left out: Tint (the project does not keep it)\n"),
        "{report}"
    );
    assert!(!report.contains("slots: 1 from the controls"), "{report}");
}

#[test]
fn without_a_panel_the_project_imports_as_before() {
    let import = imported(Options {
        comp: Some("Main".into()),
        ..Options::default()
    });
    assert_eq!(rotation(&import, "l1"), json!(30));
    assert_eq!(import.mapping.panel, None);
    assert_eq!(
        import.json["slots"],
        json!({"speed": {"number": 4, "label": "Speed"}})
    );
}

#[test]
fn a_control_nothing_follows_is_left_out_with_why() {
    let import = imported_reading(
        Options {
            panel: panel(),
            ..Options::default()
        },
        "time * 10",
    );
    assert_eq!(
        import.json["slots"],
        json!({"turn": {"number": 30, "label": "Motion / Turn"}})
    );
    let report = import.mapping.to_string();
    assert!(
        report.contains("  template: 3 controls, 1 slot\n"),
        "{report}"
    );
    assert!(
        report.contains("    left out: Pace (nothing the scene draws follows it)\n"),
        "{report}"
    );
}

#[test]
fn a_transform_value_left_out_at_its_default_follows_its_slot() {
    // The project stores no transform for `L3`: its position is the
    // composition's centre, 100 by 50.
    let mut main = comp(vec![layer(3, "L3", vec![])], 2.0);
    main.panel = vec![PanelControl {
        id: "pos".into(),
        kind: ControlKind::Point,
        targets: vec![ControlTarget {
            composition: 5,
            layer: 3,
            path: vec![
                step("ADBE Transform Group", None),
                step("ADBE Position", None),
            ],
        }],
    }];
    let import = import(
        &project(vec![(5, "Main", main)]),
        &Options {
            panel: vec![offered(
                "pos",
                ControlKind::Point,
                "Place",
                json!({"x": 100, "y": 50}),
            )],
            ..Options::default()
        },
    )
    .expect("it imports");
    assert_eq!(
        import.json["slots"],
        json!({"place": {"point": [100, 50], "label": "Place"}})
    );
    assert_eq!(import.json["layers"][0]["position"], json!("$place"));
}

/// The local sweep: every sample template imports with its panel. Prints
/// counts and the reasons controls are left out, never their names.
///
/// ```text
/// MOTION_AEP_SAMPLES=~/aep-samples cargo test -p motion-import every_sample_template_imports -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs $MOTION_AEP_SAMPLES"]
fn every_sample_template_imports_with_its_panel() {
    let dir = std::env::var("MOTION_AEP_SAMPLES").expect("set $MOTION_AEP_SAMPLES");
    let mut stack = vec![std::path::PathBuf::from(dir)];
    let (mut files, mut controls, mut slots) = (0, 0, 0);
    let mut why = std::collections::BTreeMap::<String, usize>::new();
    let mut followers = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mogrt"))
            {
                continue;
            }
            let template =
                crate::template::open(&std::fs::read(&path).expect("read")).expect("opens");
            let project = crate::decode(&template.project).expect("decodes");
            let import = import(
                &project,
                &Options {
                    panel: template.panel,
                    ..Options::default()
                },
            )
            .expect("imports");
            files += 1;
            let panel = import.mapping.panel.clone().expect("a panel report");
            controls += panel.controls;
            slots += panel.slots;
            for left in panel.left_out {
                *why.entry(left.what).or_default() += 1;
            }
            followers += import.json.to_string().matches("\"$").count();
        }
    }
    println!(
        "{files} templates: {controls} controls, {slots} slots, {followers} references; left out: {why:?}"
    );
}

/// Writes each local template's scene to `$MOTION_OUT/<n>.json`, for a
/// look, with the fonts of `$MOTION_FONTS` when it is set.
#[test]
#[ignore = "needs $MOTION_AEP_SAMPLES and $MOTION_OUT"]
fn write_sample_template_scenes() {
    let dir = std::env::var("MOTION_AEP_SAMPLES").expect("set");
    let out = std::path::PathBuf::from(std::env::var("MOTION_OUT").expect("set"));
    let fonts: Vec<String> = std::env::var("MOTION_FONTS").into_iter().collect();
    let mut stack = vec![std::path::PathBuf::from(&dir)];
    let mut found = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mogrt"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    for (n, path) in found.iter().enumerate() {
        let template = crate::template::open(&std::fs::read(path).unwrap()).unwrap();
        let project = crate::decode(&template.project).unwrap();
        let import = import(
            &project,
            &Options {
                panel: template.panel,
                fonts: fonts.clone(),
                ..Options::default()
            },
        )
        .unwrap();
        let slots = import.json["slots"]
            .as_object()
            .map_or(0, serde_json::Map::len);
        std::fs::write(
            out.join(format!("{n}.json")),
            serde_json::to_string_pretty(&import.json).unwrap(),
        )
        .unwrap();
        println!(
            "{n}: {slots} slots, {}x{}",
            import.json["size"], import.json["dur"]
        );
    }
}
