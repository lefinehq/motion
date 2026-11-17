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
    assert_eq!(what, ["`ADBE Layer Source Alternate`"]);
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

/// A layer that runs backwards keeps its in point after its out point,
/// and shows between the two; one whose points meet never shows.
#[test]
fn a_layer_running_backwards_shows_between_its_points() {
    use crate::test_support::build::{comp, layer, project, solid};
    let timed = |id: u32, (in_point, out_point): (f64, f64), stretch: f64| {
        let mut timed = layer(id, &format!("Layer {id}"), vec![]);
        timed.source = Some(9);
        (timed.in_point, timed.out_point) = (in_point, out_point);
        (timed.start_time, timed.stretch) = (in_point, stretch);
        timed
    };
    let mut project = project(vec![(
        1,
        "Main",
        comp(
            vec![
                timed(2, (3.0, 1.0), -1.0),
                timed(3, (1.0, 3.0), 1.0),
                timed(4, (2.0, 2.0), -1.0),
            ],
            4.0,
        ),
    )]);
    project.items.push(solid(9, 20, 10));
    let import = import(&project, &Options::default()).expect("it imports");
    let timing = |i: usize| {
        let layer = &import.json["layers"][i];
        ["span", "offset", "stretch", "hidden"].map(|key| layer[key].clone())
    };
    // Layers are written top first: the last one listed is drawn first.
    assert_eq!(
        timing(2),
        [json!("1s-3s"), json!("3s"), json!(-1), Json::Null]
    );
    assert_eq!(
        timing(1),
        [json!("1s-3s"), json!("1s"), Json::Null, Json::Null]
    );
    assert_eq!(timing(0)[3], json!(true));
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

/// Effects: the transform effect, points in fractions of the layer's effect
/// space, pseudo effects and effects that draw in place of their layer.
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
        defined(effects, vec![("ADBE Geometry2", geometry())])
    }

    /// Like [`imported`], with the project keeping `definitions`.
    fn defined(
        effects: Vec<Group>,
        definitions: Vec<(&str, EffectDefinition)>,
    ) -> motion_format::Result<Import> {
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
        for (name, definition) in definitions {
            project.effects.insert(name.into(), definition);
        }
        import(&project, &Options::default())
    }

    /// Like [`imported`], with the shape layer's adjustment switch on.
    fn adjusting_shape(effects: Vec<Group>) -> Import {
        let mut shape = layer(1, "Shape", vec![]);
        shape.kind = LayerKind::Shape;
        shape.switches.adjustment = true;
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
        import(&project, &Options::default()).expect("imports")
    }

    #[test]
    fn a_shape_adjustment_layer_adjusts_where_its_shapes_are() {
        let import = adjusting_shape(vec![transform(vec![number("ADBE Geometry2-0007", 90.0)])]);
        let layer = &import.json["layers"][0];
        assert_eq!(layer["adjust"], true);
        assert!(layer["shapes"].is_array(), "{layer}");
        assert_eq!(layer["fx"][0]["rotate"], 90);
    }

    #[test]
    fn a_shape_adjustment_layer_without_a_drawn_effect_draws_nothing() {
        let import = adjusting_shape(vec![group("ADBE Lens Flare", vec![])]);
        let layer = &import.json["layers"][0];
        assert_eq!(layer["pivot"], true);
        assert!(
            layer.get("shapes").is_none() && layer.get("adjust").is_none(),
            "{layer}"
        );
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

    /// A shape layer's effect space is a box the size of its composition
    /// with the layer's origin in the middle, so the default anchor, the
    /// middle of that box, is the origin, the scene's own rest.
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
            json!([{"transform":1.5,"position":[-50,0],"rotate":30,"opacity":0.5}])
        );
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
    }

    /// A layer with a source measures its effects' points from the
    /// source's top left, in fractions of the source's size.
    #[test]
    fn a_layer_with_a_source_takes_its_points_from_the_top_left() {
        let mut nested = layer(1, "Nested", vec![]);
        nested.source = Some(3);
        nested.properties.children = vec![Child::Group(group(
            "ADBE Effect Parade",
            vec![Child::Group(transform(vec![property(
                "ADBE Geometry2-0002",
                Stream::Static(Value::Vector2([0.25, 0.5])),
            )]))],
        ))];
        let mut project = project(vec![
            (2, "Main", comp(vec![nested], 2.0)),
            (3, "Inner", comp(vec![], 2.0)),
        ]);
        project.effects.insert("ADBE Geometry2".into(), geometry());
        let import = import(&project, &Options::default()).expect("imports");
        assert_eq!(
            import.json["layers"][0]["fx"],
            json!([{"transform":[1,1],"anchor":[100,50],"position":[50,50]}])
        );
    }

    /// A collapsed composition draws into the one it sits in, and its
    /// effects measure their points in that one's box, from its top left.
    #[test]
    fn a_collapsed_composition_takes_its_points_in_its_parents_box() {
        let mut nested = layer(1, "Nested", vec![]);
        nested.source = Some(3);
        nested.switches.collapse = true;
        nested.properties.children = vec![Child::Group(group(
            "ADBE Effect Parade",
            vec![Child::Group(transform(vec![property(
                "ADBE Geometry2-0002",
                Stream::Static(Value::Vector2([0.25, 0.5])),
            )]))],
        ))];
        let mut inner = comp(vec![], 2.0);
        (inner.width, inner.height) = (40, 20);
        let mut project = project(vec![
            (2, "Main", comp(vec![nested], 2.0)),
            (3, "Inner", inner),
        ]);
        project.effects.insert("ADBE Geometry2".into(), geometry());
        let import = import(&project, &Options::default()).expect("imports");
        let layer = &import.json["layers"][0];
        assert_eq!(layer["collapse"], true);
        assert_eq!(
            layer["fx"],
            json!([{"transform":[1,1],"anchor":[100,50],"position":[50,50]}])
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
        for effect in ["CC Particle World", "CC Star Burst"] {
            let import = imported(vec![transform(vec![]), group(effect, vec![])]).expect("imports");
            let layer = &import.json["layers"][0];
            assert_eq!(layer["pivot"], true);
            assert!(
                layer.get("shapes").is_none() && layer.get("fx").is_none(),
                "{layer}"
            );
            let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
            let drawn = format!("layers whose `{effect}` draws in their place (drawn as nothing)");
            assert!(what.contains(&drawn.as_str()), "{what:?}");
            assert!(what.contains(&format!("`{effect}`").as_str()), "{what:?}");
        }
    }

    /// The channel menu and the sliders of a levels effect only show the
    /// levels its data keeps: without data, the effect changes nothing.
    #[test]
    fn a_levels_panel_draws_nothing_of_its_own() {
        let import = imported(vec![group(
            "ADBE Easy Levels2",
            vec![
                Child::Property(number("ADBE Easy Levels2-0001", 2.0)),
                Child::Property(number("ADBE Easy Levels2-0005", 1.5)),
            ],
        )])
        .expect("imports");
        assert_eq!(import.json["layers"][0]["fx"], json!([{"levels":1}]));
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
    }

    /// A laser is a beam: its length and time as they are, its thickness in
    /// px, and a beam without 3D perspective reported.
    #[test]
    fn a_laser_is_a_beam() {
        let import = imported(vec![group(
            "ADBE Laser",
            vec![
                Child::Property(number("ADBE Laser-0003", 1.0)),
                Child::Property(number("ADBE Laser-0004", 0.5)),
                Child::Property(number("ADBE Laser-0005", 10.0)),
                Child::Property(number("ADBE Laser-0010", 0.0)),
                Child::Property(number("ADBE Laser-0011", 1.0)),
            ],
        )])
        .expect("imports");
        let fx = &import.json["layers"][0]["fx"][0];
        assert_eq!(fx["beam"], 0.5, "{fx}");
        assert_eq!(fx["length"], 1.0, "{fx}");
        assert_eq!(fx["start-thickness"], 10.0, "{fx}");
        assert_eq!(fx["composite"], "on", "{fx}");
        assert!(import.json["layers"][0].get("pivot").is_none());
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert!(
            what.contains(&"a beam drawn flat, without 3D perspective"),
            "{what:?}"
        );
    }

    /// Tiles keep their reach in px and their tiling; a tiling the scene
    /// does not draw, and tiles blended across their borders, are
    /// reported.
    #[test]
    fn repeated_tiles_take_their_reach_and_tiling() {
        let tiles = |properties: Vec<Property>| {
            group(
                "CC RepeTile",
                properties.into_iter().map(Child::Property).collect(),
            )
        };
        let import = imported(vec![
            tiles(vec![
                number("CC RepeTile-0001", 743.0),
                number("CC RepeTile-0002", 741.0),
            ]),
            tiles(vec![
                number("CC RepeTile-0003", 500.0),
                number("CC RepeTile-0004", 400.0),
                number("CC RepeTile-0005", 17.0),
            ]),
            tiles(vec![
                number("CC RepeTile-0001", 30.0),
                number("CC RepeTile-0005", 11.0),
            ]),
            tiles(vec![
                number("CC RepeTile-0005", 9.0),
                number("CC RepeTile-0006", 0.5),
            ]),
        ])
        .expect("imports");
        let fx = &import.json["layers"][0]["fx"];
        assert_eq!(
            fx[0],
            json!({"repeat-tile":"repeat","right":743,"left":741})
        );
        assert_eq!(fx[1], json!({"repeat-tile":"brick","down":500,"up":400}));
        assert_eq!(fx[2], json!({"repeat-tile":"none","right":30}));
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert_eq!(
            what,
            [
                "a `tiling` the scene does not have",
                "tiles blended across their borders"
            ]
        );
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

    /// A mirror's centre is a point of the layer's effect space; one the
    /// project leaves out is its definition's, the right edge's middle.
    #[test]
    fn a_mirror_takes_its_centre_in_pixels_and_its_angle_in_degrees() {
        let definition = EffectDefinition {
            name: "Mirror".into(),
            parameters: vec![
                parameter("ADBE Mirror-0000", Control::Layer),
                parameter(
                    "ADBE Mirror-0001",
                    Control::Point {
                        default: vec![1.0, 0.5],
                    },
                ),
                parameter("ADBE Mirror-0002", Control::Angle { default: 0.0 }),
            ],
        };
        let import = defined(
            vec![
                group(
                    "ADBE Mirror",
                    vec![
                        Child::Property(property(
                            "ADBE Mirror-0001",
                            Stream::Static(Value::Vector2([0.25, 0.5])),
                        )),
                        Child::Property(number("ADBE Mirror-0002", 90.0)),
                    ],
                ),
                group("ADBE Mirror", vec![]),
            ],
            vec![("ADBE Mirror", definition)],
        )
        .expect("imports");
        assert_eq!(
            import.json["layers"][0]["fx"],
            json!([{"mirror":[-50,0],"angle":90},{"mirror":[100,0]}])
        );
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
    }

    #[test]
    fn a_displacement_map_names_its_map_layer() {
        let map = |layer: u32| {
            group(
                "ADBE Displacement Map",
                vec![
                    Child::Property(property(
                        "ADBE Displacement Map-0001",
                        Stream::Static(Value::Layer(layer)),
                    )),
                    Child::Property(number("ADBE Displacement Map-0003", 20.0)),
                ],
            )
        };
        let mut shape = layer(1, "Shape", vec![]);
        shape.kind = LayerKind::Shape;
        shape.properties.children = vec![Child::Group(group(
            "ADBE Effect Parade",
            vec![Child::Group(map(7)), Child::Group(map(1))],
        ))];
        let mut noise = layer(7, "Noise", vec![]);
        noise.kind = LayerKind::Shape;
        let project = project(vec![(2, "Main", comp(vec![shape, noise], 2.0))]);
        let import = import(&project, &Options::default()).expect("imports");
        let shape = import.json["layers"]
            .as_array()
            .and_then(|layers| layers.iter().find(|l| l["id"] == "shape"))
            .expect("the shape layer");
        assert_eq!(shape["fx"][0]["map"], "noise", "{shape}");
        assert_eq!(shape["fx"][0]["displacement-map"], 20, "{shape}");
        // A map that is the layer itself is what the effect reads anyway.
        assert!(shape["fx"][1].get("map").is_none(), "{shape}");
    }

    /// A conversion the project leaves out is its definition's default,
    /// which is not the scene's.
    #[test]
    fn polar_coordinates_take_their_conversion_from_the_definition() {
        let definition = EffectDefinition {
            name: "Polar Coordinates".into(),
            parameters: vec![
                parameter("ADBE Polar Coordinates-0000", Control::Layer),
                parameter(
                    "ADBE Polar Coordinates-0001",
                    Control::Slider {
                        default: 0.0,
                        min: Some(0.0),
                        max: Some(1.0),
                        percent: false,
                    },
                ),
                parameter(
                    "ADBE Polar Coordinates-0002",
                    Control::Popup {
                        default: 2,
                        choices: vec!["Rect to Polar".into(), "Polar to Rect".into()],
                    },
                ),
            ],
        };
        let polar = |properties: Vec<Property>| {
            group(
                "ADBE Polar Coordinates",
                properties.into_iter().map(Child::Property).collect(),
            )
        };
        let import = defined(
            vec![
                polar(vec![
                    number("ADBE Polar Coordinates-0001", 0.81),
                    number("ADBE Polar Coordinates-0002", 1.0),
                ]),
                polar(vec![number("ADBE Polar Coordinates-0001", 1.0)]),
            ],
            vec![("ADBE Polar Coordinates", definition)],
        )
        .expect("imports");
        assert_eq!(
            import.json["layers"][0]["fx"],
            json!([
                {"polar-coordinates":0.81,"conversion":"rect-to-polar"},
                {"polar-coordinates":1,"conversion":"polar-to-rect"}
            ])
        );
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
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

    #[test]
    fn levels_come_from_the_data_kept_for_them() {
        use crate::map::levels::IDENTITY;
        let rgb = [0.137_255, 1.0, 2.1, 0.0, 1.0];
        let red = [0.066_667, 1.0, 1.14, 0.0, 1.0];
        let blue = [0.0, 0.8, 1.0, 0.1, 1.0];
        let data = crate::map::levels::tests::data(2, [rgb, red, IDENTITY, blue, IDENTITY]);
        let levels = |stream, enabled| {
            let mut effect = group(
                "ADBE Easy Levels2",
                vec![
                    Child::Property(property(
                        "ADBE Easy Levels2-0001",
                        Stream::Static(Value::Number(2.0)),
                    )),
                    Child::Property(property("ADBE Easy Levels2-0002", stream)),
                    // The sliders show the panel's channel as it last
                    // showed it, which the data overrules.
                    Child::Property(property(
                        "ADBE Easy Levels2-0003",
                        Stream::Static(Value::Number(0.5)),
                    )),
                ],
            );
            effect.enabled = enabled;
            imported(vec![effect]).expect("imports")
        };
        let import = levels(Stream::Static(Value::Data(data.clone())), true);
        assert_eq!(
            import.json["layers"][0]["fx"],
            json!([
                {"levels":1.14,"input-black":0.066667,"channel":"red"},
                {"levels":1,"input-white":0.8,"output-black":0.1,"channel":"blue"},
                {"levels":2.1,"input-black":0.137255}
            ])
        );
        assert!(
            import.mapping.unsupported.is_empty(),
            "{:?}",
            import.mapping.unsupported
        );
        // An effect turned off is off in every step.
        let off = levels(Stream::Static(Value::Data(data.clone())), false);
        let hidden: Vec<_> = off.json["layers"][0]["fx"]
            .as_array()
            .expect("effects")
            .iter()
            .map(|item| item["hidden"].clone())
            .collect();
        assert_eq!(hidden, vec![json!(true); 3]);
        let unread = levels(Stream::Static(Value::Data(data[..100].to_vec())), true);
        let what: Vec<&str> = unread.mapping.by_what().into_keys().collect();
        assert!(
            what.contains(&"levels kept in a layout the import does not read"),
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
    fn a_null_whose_anchor_the_project_leaves_out_rests_at_its_corner() {
        // A null over a 100 by 100 solid at the composition's centre, and a
        // child at its origin: the child sits where the null's position is.
        let mut null = layer(1, "Null 1", vec![]);
        null.source = Some(9);
        null.switches.null = true;
        let mut child = layer(2, "Child", vec![]);
        child.parent = Some(1);
        child.properties.children = vec![crate::test_support::build::transform(vec![
            crate::test_support::build::property(
                "ADBE Position",
                crate::model::Stream::Static(crate::model::Value::Vector3([0.0, 0.0, 0.0])),
            ),
        ])];
        let mut project = project(vec![(2, "Main", comp(vec![child, null], 2.0))]);
        project.items.push(crate::model::Item {
            id: 9,
            name: "Null 1".into(),
            folder: None,
            kind: crate::model::ItemKind::Footage(crate::model::Footage {
                width: 100,
                height: 100,
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
        let import = import(&project, &Options::default()).expect("imports");
        let null = import.json["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["pivot"] == true)
            .expect("the null");
        assert!(
            null.get("anchor").is_none_or(|a| *a == json!([0, 0])),
            "{null}"
        );
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
        let import = adjusting(vec![group("ADBE Lens Flare", vec![])]);
        let layer = &import.json["layers"][0];
        assert_eq!(layer["pivot"], true);
        assert!(
            layer.get("rect").is_none() && layer.get("adjust").is_none(),
            "{layer}"
        );
        let what: Vec<&str> = import.mapping.by_what().into_keys().collect();
        assert_eq!(what, ["`ADBE Lens Flare`"]);
    }
}

/// The scene layer called `name` of `import`.
fn scene_layer<'i>(import: &'i Import, name: &str) -> &'i serde_json::Value {
    import.json["layers"]
        .as_array()
        .expect("layers")
        .iter()
        .find(|l| l["id"] == name)
        .unwrap_or_else(|| panic!("no layer {name}"))
}

#[test]
fn a_layer_with_a_source_keeps_its_anchor_in_fractions_of_it() {
    use crate::model::{Ease, Interpolation, Keyframe, LayerKind, Spatial, Stream, Value};
    use crate::test_support::build::{comp, layer, project, property, solid, transform};
    use serde_json::json;
    let anchor = |stream| transform(vec![property("ADBE Anchor Point", stream)]);
    let at = |x, y| Stream::Static(Value::Vector3([x, y, 0.0]));
    let key = |time, x, y, tangent: [f64; 2]| Keyframe {
        time,
        value: Value::Vector3([x, y, 0.0]),
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
        spatial: Some(Spatial {
            in_tangent: vec![-tangent[0], -tangent[1], 0.0],
            out_tangent: vec![tangent[0], tangent[1], 0.0],
            continuous: false,
            auto_bezier: false,
        }),
        continuous: false,
        auto_bezier: false,
        roving: false,
    };
    // A 300 by 200 solid anchored at a quarter of its width and its foot,
    // the same solid with keys and a bent path, a null over a 100 by 100
    // solid at its middle, and a shape layer, whose anchor is in pixels.
    let mut sized = layer(1, "Sized", vec![anchor(at(0.25, 1.0))]);
    sized.source = Some(9);
    let mut keyed = layer(
        2,
        "Keyed",
        vec![anchor(Stream::Animated(vec![
            key(0.0, 0.0, 0.0, [0.1, 0.0]),
            key(1.0, 1.0, 1.0, [0.0, 0.5]),
        ]))],
    );
    keyed.source = Some(9);
    let mut null = layer(3, "Null", vec![anchor(at(0.5, 0.5))]);
    null.source = Some(8);
    null.switches.null = true;
    let mut shape = layer(4, "Shape", vec![anchor(at(10.0, 20.0))]);
    shape.kind = LayerKind::Shape;
    let mut left_out = layer(5, "Left out", vec![]);
    left_out.source = Some(9);
    let mut project = project(vec![(
        2,
        "Main",
        comp(vec![sized, keyed, null, shape, left_out], 2.0),
    )]);
    project
        .items
        .extend([solid(9, 300, 200), solid(8, 100, 100)]);
    let import = import(&project, &Options::default()).expect("imports");
    assert_eq!(scene_layer(&import, "sized")["anchor"], json!([75, 200]));
    assert_eq!(scene_layer(&import, "null")["anchor"], json!([50, 50]));
    assert_eq!(scene_layer(&import, "shape")["anchor"], json!([10, 20]));
    assert_eq!(
        scene_layer(&import, "left-out")["anchor"],
        json!([150, 100])
    );
    let keys = &scene_layer(&import, "keyed")["anim"]["anchor"];
    assert_eq!(keys[0]["v"], json!([0, 0]));
    assert_eq!(keys[1]["v"], json!([300, 200]));
    // The path's handles are in pixels of the source too.
    assert_eq!(keys[0]["path"], json!([[30, 0], [0, -100]]));
}

#[test]
fn a_blend_mode_the_scene_has_is_written_and_another_reported() {
    use crate::BlendMode;
    let imported = |mode| {
        let mut sample = sample_project();
        for item in &mut sample.items {
            if let crate::model::ItemKind::Composition(c) = &mut item.kind {
                c.layers
                    .iter_mut()
                    .for_each(|layer| layer.blend_mode = mode);
            }
        }
        import(&sample, &Options::default()).expect("the sample imports")
    };
    let screen = imported(BlendMode::Screen);
    assert_eq!(screen.json["layers"][0]["blend"], "screen");
    let dissolve = imported(BlendMode::Dissolve);
    assert!(dissolve.json["layers"][0].get("blend").is_none());
    assert!(
        dissolve
            .mapping
            .unsupported
            .iter()
            .any(|f| f.what == "the blend mode `dissolve`")
    );
}

#[test]
fn motion_blur_follows_the_switches_and_the_composition_keeps_its_shutter() {
    use crate::model::MotionBlur;
    use crate::test_support::build::{comp, layer, project};
    let imported = |on: bool| {
        let mut blurred = layer(1, "Ball", vec![]);
        blurred.switches.motion_blur = true;
        let main = crate::model::Composition {
            motion_blur: MotionBlur {
                on,
                angle: 360.0,
                phase: -180.0,
                samples: 8,
            },
            ..comp(vec![blurred], 2.0)
        };
        import(&project(vec![(5, "Main", main)]), &Options::default()).expect("it imports")
    };
    let on = imported(true);
    assert_eq!(on.json["shutter"], "360 -180 8");
    assert_eq!(on.json["layers"][0]["mblur"], true);
    let off = imported(false);
    assert!(off.json.get("shutter").is_none());
    assert!(off.json["layers"][0].get("mblur").is_none());
    assert!(
        off.mapping.unsupported.is_empty(),
        "{:?}",
        off.mapping.unsupported
    );
}

#[test]
fn a_star_maps_its_rows_settings_and_defaults_and_reports_an_unknown_direction() {
    use crate::model::{Child, LayerKind};
    use crate::test_support::build::{comp, group, layer, number, point, project, property};
    let star = |properties: Vec<(&str, crate::model::Stream)>| {
        Child::Group(group(
            "ADBE Vector Shape - Star",
            properties
                .into_iter()
                .map(|(name, value)| Child::Property(property(name, value)))
                .collect(),
        ))
    };
    let mut shape = layer(1, "Shape", vec![]);
    shape.kind = LayerKind::Shape;
    shape.properties.children = vec![Child::Group(group(
        "ADBE Root Vectors Group",
        vec![
            star(vec![
                ("ADBE Vector Star Type", number(2.0)),
                ("ADBE Vector Star Points", number(6.0)),
                ("ADBE Vector Star Position", point(3.0, -4.0)),
                ("ADBE Vector Star Rotation", number(30.0)),
                ("ADBE Vector Star Inner Radius", number(20.0)),
                ("ADBE Vector Star Outer Radius", number(80.0)),
                ("ADBE Vector Star Inner Roundess", number(10.0)),
                ("ADBE Vector Star Outer Roundess", number(67.0)),
                ("ADBE Vector Shape Direction", number(3.0)),
            ]),
            star(vec![]),
            star(vec![("ADBE Vector Shape Direction", number(2.0))]),
            Child::Group(group("ADBE Vector Graphic - Fill", vec![])),
        ],
    ))];
    let import = import(
        &project(vec![(2, "Main", comp(vec![shape], 2.0))]),
        &Options::default(),
    )
    .unwrap();
    let shapes = &import.json["layers"][0]["shapes"];
    assert_eq!(
        shapes[0],
        serde_json::json!({
            "star": 6, "type": "polygon", "position": [3, -4], "rotate": 30,
            "inner-radius": 20, "outer-radius": 80, "inner-roundness": 0.1,
            "outer-roundness": 0.67, "direction": "counterclockwise"
        })
    );
    // Left out, a star is the original's default: five points, 100 and 50 px.
    assert_eq!(shapes[1], serde_json::json!({ "star": 5 }));
    let found: Vec<(&str, &str)> = import
        .mapping
        .unsupported
        .iter()
        .map(|f| (f.at.as_str(), f.what.as_str()))
        .collect();
    assert_eq!(
        found,
        [(
            "Main/Shape/ADBE Root Vectors Group/ADBE Vector Shape - Star/ADBE Vector Shape Direction",
            "a `direction` the scene does not have"
        )]
    );
}
