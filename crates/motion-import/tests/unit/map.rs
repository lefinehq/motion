//! Unit tests for `src/map.rs`: a whole import of the sample project, and
//! how the composition to import is chosen.

use pretty_assertions::assert_eq;

use super::*;
use crate::test_support::sample_project;

/// The sample project imported with `options`.
fn imported(options: &Options) -> Import {
    import(&sample_project(), options).expect("the sample imports")
}

#[test]
fn imports_the_sample_with_its_report() {
    let import = imported(&Options {
        comp: None,
        fonts: vec!["fonts".into()],
        ..Options::default()
    });
    let scene = motion_format::write_document(&import.json);
    insta::assert_snapshot!(format!("{scene}\n{}", import.mapping));
}

#[test]
fn layers_stack_bottom_first_and_keep_their_parent() {
    let import = imported(&Options::default());
    let layers = import.document.scene().root.layers.clone();
    let ids: Vec<&str> = layers.iter().map(|layer| layer.id.as_str()).collect();
    assert_eq!(ids, ["ctrl", "title"]);
    assert_eq!(layers[1].parent.as_deref(), Some("ctrl"));
    let named: Vec<(&str, &str)> = import
        .mapping
        .ids
        .iter()
        .map(|named| (named.name.as_str(), named.id.as_str()))
        .collect();
    assert_eq!(
        named,
        [("Main", "main"), ("Title", "title"), ("Ctrl", "ctrl")]
    );
}

#[test]
fn reports_what_the_scene_cannot_hold() {
    let mapping = imported(&Options::default()).mapping;
    let what: Vec<&str> = mapping.by_what().into_keys().collect();
    assert_eq!(what, ["3D layers (drawn flat)", "`ADBE Marker`"]);
    assert_eq!(mapping.counts["ADBE Position"].mapped, 1);
    assert_eq!(mapping.counts["ADBE Slider Control"].ignored, 1);
}

#[test]
fn chooses_the_composition_by_name_or_refuses_to_guess() {
    let mut project = sample_project();
    let second = project.items[1].clone();
    project.items.push(Item {
        id: 4,
        name: "Alt".into(),
        ..second
    });
    let error = import(&project, &Options::default()).unwrap_err();
    assert_eq!(error.kind, Kind::Conflict);
    assert!(error.expected.is_some());
    let chosen = Options {
        comp: Some("Alt".into()),
        ..Options::default()
    };
    assert_eq!(import(&project, &chosen).unwrap().mapping.scene, "Alt");
    let unknown = Options {
        comp: Some("Nope".into()),
        ..Options::default()
    };
    assert_eq!(
        import(&project, &unknown).unwrap_err().kind,
        Kind::UnknownName
    );
}

#[test]
fn a_project_without_compositions_says_so() {
    let mut project = sample_project();
    project
        .items
        .retain(|item| !matches!(item.kind, ItemKind::Composition(_)));
    let error = import(&project, &Options::default()).unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert_eq!(error.message, "the project holds no composition to import");
}

/// What the import of one comp holding `layers` reports, as `(at, what)`.
fn findings(layers: Vec<crate::model::Layer>) -> Vec<(String, String)> {
    use crate::test_support::build::{comp, project};
    let project = project(vec![(1, "Main", comp(layers, 2.0))]);
    import(&project, &Options::default())
        .expect("it imports")
        .mapping
        .unsupported
        .into_iter()
        .map(|finding| (finding.at, finding.what))
        .collect()
}

#[test]
fn a_layer_whose_source_is_missing_is_reported() {
    use crate::test_support::build::layer;
    let mut orphan = layer(2, "Orphan", vec![]);
    orphan.source = Some(99);
    let found = findings(vec![orphan]);
    assert_eq!(
        found,
        [(
            "Main/Orphan".to_owned(),
            "a source the project does not hold (drawn as nothing)".to_owned()
        )]
    );
}

#[test]
fn a_parent_outside_the_composition_is_reported() {
    use crate::test_support::build::layer;
    let mut child = layer(2, "Child", vec![]);
    child.parent = Some(99);
    let found = findings(vec![child]);
    assert_eq!(
        found,
        [(
            "Main/Child".to_owned(),
            "a parent that is not in its composition (drawn without a parent)".to_owned()
        )]
    );
}

/// The sample project with a second composition called `name`, item id 4,
/// 300 wide where the first is not.
fn with_second(name: &str) -> Project {
    let mut project = sample_project();
    let mut second = project.items[1].clone();
    if let ItemKind::Composition(composition) = &mut second.kind {
        composition.width = 300;
    }
    project.items.push(Item {
        id: 4,
        name: name.into(),
        ..second
    });
    project
}

/// The width of the scene `options` imports from `project`.
fn width(project: &Project, options: &Options) -> String {
    let import = import(project, options).unwrap();
    import.json["size"]
        .as_str()
        .unwrap()
        .split('x')
        .next()
        .unwrap()
        .to_owned()
}

/// `--comp` set to `comp`.
fn comp(comp: &str) -> Options {
    Options {
        comp: Some(comp.into()),
        ..Options::default()
    }
}

#[test]
fn a_name_two_compositions_share_is_refused_and_an_id_chooses() {
    let project = with_second("Main");
    let error = import(&project, &comp("Main")).unwrap_err();
    assert_eq!(error.kind, Kind::Conflict);
    assert_eq!(
        error.expected.as_deref(),
        Some("one of them by its item id (`--comp <id>`)")
    );
    let first = project.items[1].id.to_string();
    assert_ne!(width(&project, &comp(&first)), "300");
    assert_eq!(width(&project, &comp("4")), "300");
}

#[test]
fn a_name_wins_over_an_id() {
    let project = with_second("4");
    assert_eq!(width(&project, &comp("4")), "300");
    let first = project.items[1].id.to_string();
    assert_ne!(width(&project, &comp(&first)), "300");
}

#[test]
fn a_stored_value_outside_the_range_is_clamped_and_reported() {
    use crate::test_support::build::{comp, layer, number, project, property, transform};
    let title = layer(
        1,
        "Title",
        vec![transform(vec![property("ADBE Opacity", number(1.5))])],
    );
    let import = import(
        &project(vec![(5, "Main", comp(vec![title], 2.0))]),
        &Options::default(),
    )
    .expect("a value outside the range still imports");
    assert_eq!(import.json["layers"][0]["opacity"], serde_json::json!(1));
    assert_eq!(import.mapping.clamped.len(), 1);
    assert_eq!(import.mapping.clamped[0].what, "1 value outside 0..=1");
    assert!(
        import
            .mapping
            .to_string()
            .contains("clamped into the scene's range")
    );
}

/// Effects: the transform effect, points in fractions of the layer's size,
/// pseudo effects and effects that draw in place of their layer.
mod effects {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::{Import, Options, import};
    use crate::model::{
        Child, Control, EffectDefinition, Group, LayerKind, Parameter, Property, Stream, Value,
    };
    use crate::test_support::build::{comp, group, layer, project, property};

    /// A parameter of `control`.
    fn parameter(match_name: &str, control: Control) -> Parameter {
        Parameter {
            match_name: match_name.into(),
            name: String::new(),
            invisible: false,
            animatable: true,
            control,
        }
    }

    /// The transform effect's definition, as a project keeps it.
    fn geometry() -> EffectDefinition {
        let slider = |default| Control::Slider {
            default,
            min: None,
            max: None,
            percent: false,
        };
        EffectDefinition {
            name: "Transform".into(),
            parameters: vec![
                parameter("ADBE Geometry2-0000", Control::Layer),
                parameter(
                    "ADBE Geometry2-0001",
                    Control::Point {
                        default: vec![0.5, 0.5],
                    },
                ),
                parameter(
                    "ADBE Geometry2-0002",
                    Control::Point {
                        default: vec![0.5, 0.5],
                    },
                ),
                parameter("ADBE Geometry2-0011", Control::Checkbox { default: true }),
                parameter("ADBE Geometry2-0003", slider(100.0)),
                parameter("ADBE Geometry2-0004", slider(100.0)),
                parameter("ADBE Geometry2-0007", Control::Angle { default: 0.0 }),
                parameter("ADBE Geometry2-0008", slider(100.0)),
            ],
        }
    }

    /// A 200 by 100 comp holding one shape layer with the effect groups
    /// `effects`, imported.
    fn imported(effects: Vec<Group>) -> motion_format::Result<Import> {
        let mut shape = layer(1, "Shape", vec![]);
        shape.kind = LayerKind::Shape;
        shape.properties.children = vec![
            Child::Group(group(
                "ADBE Root Vectors Group",
                vec![Child::Group(group("ADBE Vector Shape - Rect", vec![]))],
            )),
            Child::Group(group(
                "ADBE Effect Parade",
                effects.into_iter().map(Child::Group).collect(),
            )),
        ];
        let mut project = project(vec![(2, "Main", comp(vec![shape], 2.0))]);
        project.effects.insert("ADBE Geometry2".into(), geometry());
        import(&project, &Options::default())
    }

    /// The transform effect holding `properties`.
    fn transform(properties: Vec<Property>) -> Group {
        group(
            "ADBE Geometry2",
            properties.into_iter().map(Child::Property).collect(),
        )
    }

    fn number(match_name: &str, n: f64) -> Property {
        property(match_name, Stream::Static(Value::Number(n)))
    }

    #[test]
    fn the_transform_effect_takes_its_points_in_pixels_of_the_layer() {
        let import = imported(vec![transform(vec![
            property(
                "ADBE Geometry2-0002",
                Stream::Static(Value::Vector2([0.25, 0.5])),
            ),
            number("ADBE Geometry2-0003", 150.0),
            // A uniform scale's width is left as it was set apart.
            number("ADBE Geometry2-0004", 40.0),
            number("ADBE Geometry2-0007", 30.0),
            number("ADBE Geometry2-0008", 50.0),
        ])])
        .expect("imports");
        assert_eq!(
            import.json["layers"][0]["fx"],
            json!([{"transform":1.5,"anchor":[100,50],"position":[50,50],"rotate":30,"opacity":0.5}])
        );
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
    }

    #[test]
    fn a_scale_set_apart_goes_to_each_axis() {
        let import = imported(vec![transform(vec![
            number("ADBE Geometry2-0011", 0.0),
            number("ADBE Geometry2-0003", 150.0),
            number("ADBE Geometry2-0004", -100.0),
        ])])
        .expect("imports");
        let fx = &import.json["layers"][0]["fx"][0];
        assert_eq!(fx["transform"], json!([-1, 1.5]), "{fx}");
    }

    #[test]
    fn a_pseudo_effect_is_an_expression_control() {
        let import = imported(vec![group("Pseudo/MHAC PrCtrl", vec![])]).expect("imports");
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
        assert!(import.json["layers"][0].get("fx").is_none());
    }

    #[test]
    fn an_effect_drawing_in_place_of_its_layer_leaves_it_drawing_nothing() {
        let import =
            imported(vec![transform(vec![]), group("CC Particle World", vec![])]).expect("imports");
        let layer = &import.json["layers"][0];
        assert_eq!(layer["pivot"], true);
        assert!(
            layer.get("shapes").is_none() && layer.get("fx").is_none(),
            "{layer}"
        );
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert!(
            what.contains(
                &"layers whose `CC Particle World` draws in their place (drawn as nothing)"
            ),
            "{what:?}"
        );
    }

    #[test]
    fn a_levels_channel_is_its_setting() {
        let import = imported(vec![group(
            "ADBE Easy Levels2",
            vec![
                Child::Property(number("ADBE Easy Levels2-0001", 2.0)),
                Child::Property(number("ADBE Easy Levels2-0005", 1.5)),
            ],
        )])
        .expect("imports");
        let fx = &import.json["layers"][0]["fx"][0];
        assert_eq!(fx["levels"], 1.5, "{fx}");
        assert_eq!(fx["channel"], "red", "{fx}");
    }

    #[test]
    fn blurs_take_their_settings_and_levels() {
        let import = imported(vec![
            group(
                "ADBE Drop Shadow",
                vec![
                    Child::Property(number("ADBE Drop Shadow-0002", 51.0)),
                    Child::Property(number("ADBE Drop Shadow-0004", 12.0)),
                    Child::Property(number("ADBE Drop Shadow-0006", 1.0)),
                ],
            ),
            group(
                "ADBE Fast Blur",
                vec![
                    Child::Property(number("ADBE Fast Blur-0001", 20.0)),
                    Child::Property(number("ADBE Fast Blur-0002", 3.0)),
                    Child::Property(number("ADBE Fast Blur-0003", 1.0)),
                ],
            ),
            group(
                "ADBE Glo2",
                vec![
                    Child::Property(number("ADBE Glo2-0002", 102.0)),
                    Child::Property(number("ADBE Glo2-0006", 6.0)),
                ],
            ),
        ])
        .expect("imports");
        let fx = &import.json["layers"][0]["fx"];
        assert_eq!(fx[0]["drop-shadow"], 12, "{fx}");
        assert_eq!(fx[0]["opacity"], 0.2, "{fx}");
        assert_eq!(fx[0]["layer"], "hidden", "{fx}");
        assert_eq!(
            fx[1],
            json!({"gaussian-blur":20,"dimensions":"vertical","edges":"repeat"})
        );
        assert_eq!(fx[2]["threshold"], 0.4, "{fx}");
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert_eq!(what, ["a glow operation other than add"]);
    }

    #[test]
    fn curves_come_from_the_data_kept_for_them() {
        let straight: &[[u16; 2]] = &[[0, 0], [255, 255]];
        let data = crate::map::curves::tests::data([
            &[[0, 0], [128, 160], [255, 255]],
            straight,
            straight,
            &[[0, 51], [255, 255]],
            straight,
        ]);
        let curves = |stream| {
            imported(vec![group(
                "ADBE CurvesCustom",
                vec![Child::Property(property("ADBE CurvesCustom-0001", stream))],
            )])
            .expect("imports")
        };
        let import = curves(Stream::Static(Value::Data(data.clone())));
        assert_eq!(
            import.json["layers"][0]["fx"],
            json!([{"curves":"M 0,0 L 0.501961,0.627451 L 1,1","blue":"M 0,0.2 L 1,1"}])
        );
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
        let unread = curves(Stream::Static(Value::Data(data[..100].to_vec())));
        let what: Vec<&str> = unread.mapping.by_what().into_keys().collect();
        assert!(
            what.contains(&"curves kept in a layout the import does not read"),
            "{what:?}"
        );
    }

    /// A 200 by 100 comp holding a 300 by 200 adjustment solid with the
    /// effect groups `effects`, imported.
    fn adjusting(effects: Vec<Group>) -> Import {
        let mut adjustment = layer(1, "Adjust", vec![]);
        adjustment.source = Some(9);
        adjustment.switches.adjustment = true;
        adjustment.properties.children = vec![Child::Group(group(
            "ADBE Effect Parade",
            effects.into_iter().map(Child::Group).collect(),
        ))];
        let mut project = project(vec![(2, "Main", comp(vec![adjustment], 2.0))]);
        project.effects.insert("ADBE Geometry2".into(), geometry());
        project.items.push(crate::model::Item {
            id: 9,
            name: "Adjustment Layer 1".into(),
            folder: None,
            kind: crate::model::ItemKind::Footage(crate::model::Footage {
                width: 300,
                height: 200,
                pixel_aspect: 1.0,
                frame_rate: 0.0,
                duration: 0.0,
                source: crate::model::Source::Solid {
                    color: crate::model::Color {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                },
            }),
        });
        import(&project, &Options::default()).expect("imports")
    }

    #[test]
    fn an_adjustment_layer_adjusts_where_its_solid_is() {
        let import = adjusting(vec![transform(vec![number("ADBE Geometry2-0007", 90.0)])]);
        let layer = &import.json["layers"][0];
        assert_eq!(layer["adjust"], true);
        assert_eq!(layer["rect"], "300x200");
        assert_eq!(layer["fx"][0]["rotate"], 90);
        // Its points are in pixels of the solid, not the composition.
        assert_eq!(layer["fx"][0]["position"], json!([150, 100]));
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
    }

    #[test]
    fn an_adjustment_layer_without_an_effect_the_scene_draws_draws_nothing() {
        let import = adjusting(vec![group("ADBE Turbulent Displace", vec![])]);
        let layer = &import.json["layers"][0];
        assert_eq!(layer["pivot"], true);
        assert!(
            layer.get("rect").is_none() && layer.get("adjust").is_none(),
            "{layer}"
        );
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert_eq!(what, ["`ADBE Turbulent Displace`"]);
    }
}
