//! Unit tests for `src/bake/run.rs`: evaluating a project's expressions.

use pretty_assertions::assert_eq;

use super::*;
use crate::bake::{bake, bake_with};
use crate::model::{Child, ExpressionEngine, Group, Layer};
use crate::test_support::build::{
    comp, expressed, layer, number, point, project, property, slider, transform,
};

/// The bake of the property `match_name` on the layer `layer` of `project`.
fn bake_of<'b>(bakes: &'b Bakes, project: &Project, layer: &str, match_name: &str) -> &'b Bake {
    let found = project
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Composition(comp) => Some(comp),
            _ => None,
        })
        .flat_map(|comp| &comp.layers)
        .filter(|l| l.name == layer)
        .flat_map(|l| l.properties.properties())
        .find(|p| p.match_name == match_name)
        .expect("the property");
    bakes.get(found).expect("a bake")
}

fn numbers(bake: &Bake) -> (&[f64], &[Vec<f64>]) {
    match &bake.outcome {
        Ok(Samples {
            times,
            values: Sampled::Numbers { channels, .. },
            ..
        }) => (times, channels),
        other => panic!("not numbers: {other:?}"),
    }
}

/// CONTROLS with a keyed slider, and Title whose position and opacity
/// read it and their own values.
fn controlled() -> Project {
    let keyed = Stream::Animated(vec![
        crate::model::Keyframe {
            time: 0.0,
            value: Value::Number(0.0),
            in_interpolation: crate::model::Interpolation::Linear,
            out_interpolation: crate::model::Interpolation::Linear,
            in_ease: vec![],
            out_ease: vec![],
            spatial: None,
            continuous: false,
            auto_bezier: false,
            roving: false,
        },
        crate::model::Keyframe {
            time: 1.0,
            value: Value::Number(100.0),
            in_interpolation: crate::model::Interpolation::Linear,
            out_interpolation: crate::model::Interpolation::Linear,
            in_ease: vec![],
            out_ease: vec![],
            spatial: None,
            continuous: false,
            auto_bezier: false,
            roving: false,
        },
    ]);
    let controls = layer(
        1,
        "CONTROLS",
        vec![slider(
            "Speed",
            Some(property("ADBE Slider Control-0001", keyed)),
        )],
    );
    let title = layer(
        2,
        "Title",
        vec![transform(vec![
            expressed(
                "ADBE Position",
                point(10.0, 20.0),
                r#"var s = thisComp.layer("CONTROLS").effect("Speed")("Slider"); value + [s, 2 * s]"#,
            ),
            expressed("ADBE Opacity", number(0.5), "value / 2"),
            expressed("ADBE Rotate Z", number(0.0), "transform.position[0] - 10"),
        ])],
    );
    project(vec![(5, "Main", comp(vec![controls, title], 2.0))])
}

#[test]
fn expressions_bake_at_every_frame_of_their_layer() {
    let project = controlled();
    let bakes = bake(&project, Some(5));
    let (times, channels) = numbers(bake_of(&bakes, &project, "Title", "ADBE Position"));
    assert_eq!(times.len(), 20, "0 to 1.9 s at 10 frames a second");
    assert_eq!(channels.len(), 3);
    assert!(
        (channels[0][5] - 60.0).abs() < 1e-9,
        "10 + 50: {}",
        channels[0][5]
    );
    assert!((channels[1][5] - 120.0).abs() < 1e-9, "20 + 100");
    assert_eq!(
        channels[2][5], 0.0,
        "a depth it does not give stays its own"
    );
}

#[test]
fn values_return_to_the_projects_units() {
    let project = controlled();
    let bakes = bake(&project, Some(5));
    let (_, channels) = numbers(bake_of(&bakes, &project, "Title", "ADBE Opacity"));
    assert!(
        channels[0].iter().all(|&o| (o - 0.25).abs() < 1e-12),
        "50% / 2 is a quarter"
    );
}

#[test]
fn an_expression_of_values_that_never_change_runs_once() {
    let project = controlled();
    let bakes = bake(&project, Some(5));
    let constant = |m: &str| match &bake_of(&bakes, &project, "Title", m).outcome {
        Ok(samples) => samples.constant,
        Err(why) => panic!("{m}: {why}"),
    };
    assert!(constant("ADBE Opacity"), "value / 2 of a still opacity");
    assert!(!constant("ADBE Position"), "it reads a keyed slider");
    assert!(!constant("ADBE Rotate Z"), "it reads the position");
    let (times, channels) = numbers(bake_of(&bakes, &project, "Title", "ADBE Opacity"));
    assert_eq!(channels[0].len(), times.len(), "a sample at every frame");
}

#[test]
fn an_expression_reading_a_constant_one_is_constant() {
    let title = Layer {
        out_point: 3600.0,
        ..layer(
            2,
            "Title",
            vec![transform(vec![
                expressed("ADBE Opacity", number(0.5), "value * 2"),
                expressed(
                    "ADBE Rotate Z",
                    number(0.0),
                    "transform.opacity / 100 * 360",
                ),
                expressed(
                    "ADBE Position",
                    point(0.0, 0.0),
                    "[transform.rotation, time]",
                ),
            ])],
        )
    };
    let project = project(vec![(5, "Main", comp(vec![title], 3600.0))]);
    let bakes = bake(&project, Some(5));
    let samples = |m: &str| match &bake_of(&bakes, &project, "Title", m).outcome {
        Ok(samples) => samples.clone(),
        Err(why) => panic!("{m}: {why}"),
    };
    let rotation = samples("ADBE Rotate Z");
    assert!(rotation.constant);
    assert_eq!(
        rotation.times.len(),
        36_000,
        "an hour at 10 frames a second"
    );
    let position = samples("ADBE Position");
    assert!(!position.constant);
    let Sampled::Numbers { channels, .. } = &position.values else {
        panic!("numbers");
    };
    assert!(channels[0].iter().all(|&x| (x - 360.0).abs() < 1e-9));
}

#[test]
fn an_expression_reading_another_waits_for_it() {
    let project = controlled();
    let bakes = bake(&project, Some(5));
    let (_, channels) = numbers(bake_of(&bakes, &project, "Title", "ADBE Rotate Z"));
    // Position x is 10 + the slider, which runs 0 to 100 over the first second.
    assert!((channels[0][5] - 50.0).abs() < 1e-9, "{}", channels[0][5]);
}

#[test]
fn a_cycle_and_a_throw_are_reported() {
    let title = layer(
        2,
        "Title",
        vec![transform(vec![
            expressed("ADBE Rotate Z", number(0.0), "transform.opacity"),
            expressed("ADBE Opacity", number(1.0), "transform.rotation"),
            expressed("ADBE Position", point(0.0, 0.0), r#"throw new Error("no")"#),
        ])],
    );
    let project = project(vec![(5, "Main", comp(vec![title], 2.0))]);
    let bakes = bake(&project, Some(5));
    let why = |m: &str| {
        bake_of(&bakes, &project, "Title", m)
            .outcome
            .clone()
            .expect_err("fails")
    };
    assert!(
        why("ADBE Rotate Z").contains("cycle"),
        "{}",
        why("ADBE Rotate Z")
    );
    assert!(
        why("ADBE Opacity").contains("cycle"),
        "{}",
        why("ADBE Opacity")
    );
    assert!(
        why("ADBE Position").contains("it fails"),
        "{}",
        why("ADBE Position")
    );
}

#[test]
fn a_long_precomp_is_baked_only_where_the_scene_shows_it() {
    let inner = layer(
        1,
        "Spinner",
        vec![transform(vec![expressed(
            "ADBE Rotate Z",
            number(0.0),
            "time * 10",
        )])],
    );
    let inner = Layer {
        out_point: 3600.0,
        ..inner
    };
    let shown = Layer {
        source: Some(6),
        start_time: 100.0,
        ..layer(1, "Precomp", vec![])
    };
    let project = project(vec![
        (5, "Main", comp(vec![shown], 2.0)),
        (6, "Long", comp(vec![inner], 3600.0)),
    ]);
    let bakes = bake(&project, Some(5));
    // The precomp starts 100 s in, after the 2 s scene ends: nothing of it shows.
    let spinner = project.items[1].kind.clone();
    let ItemKind::Composition(long) = spinner else {
        panic!("a comp");
    };
    let rotation = long.layers[0]
        .properties
        .properties()
        .next()
        .expect("rotation");
    assert!(bakes.get(rotation).is_none());
    assert!(bakes.is_empty());
}

#[test]
fn a_precomp_shown_with_an_offset_bakes_that_span() {
    let inner = Layer {
        out_point: 3600.0,
        ..layer(
            1,
            "Spinner",
            vec![transform(vec![expressed(
                "ADBE Rotate Z",
                number(0.0),
                "time * 10",
            )])],
        )
    };
    let shown = Layer {
        source: Some(6),
        start_time: -100.0,
        ..layer(1, "Precomp", vec![])
    };
    let project = project(vec![
        (5, "Main", comp(vec![shown], 2.0)),
        (6, "Long", comp(vec![inner], 3600.0)),
    ]);
    let bakes = bake(&project, Some(5));
    let (times, channels) = numbers(bake_of(&bakes, &project, "Spinner", "ADBE Rotate Z"));
    assert_eq!(times.len(), 20);
    assert!((times[0] - 100.0).abs() < 1e-9, "{}", times[0]);
    assert!((channels[0][0] - 1000.0).abs() < 1e-6);
}

#[test]
fn source_text_bakes_as_text() {
    let document = crate::test_support::sample_document();
    let text = Child::Group(crate::test_support::build::group(
        "ADBE Text Properties",
        vec![Child::Property(expressed(
            "ADBE Text Document",
            Stream::Static(Value::Text(Box::new(document))),
            r#"thisComp.name + " " + Math.round(time * 10)"#,
        ))],
    ));
    let project = project(vec![(
        5,
        "Main",
        comp(vec![layer(1, "Label", vec![text])], 2.0),
    )]);
    let bakes = bake(&project, Some(5));
    match &bake_of(&bakes, &project, "Label", "ADBE Text Document").outcome {
        Ok(Samples {
            values: Sampled::Text(texts),
            ..
        }) => {
            assert_eq!(texts[0], "Main 0");
            assert_eq!(texts[7], "Main 7");
        }
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn a_legacy_project_repairs_its_forms_and_says_so() {
    let mut project = project(vec![(
        5,
        "Main",
        comp(
            vec![layer(
                1,
                "Title",
                vec![transform(vec![expressed(
                    "ADBE Rotate Z",
                    number(0.0),
                    "if (time > 1) 90 else 0",
                )])],
            )],
            2.0,
        ),
    )]);
    project.expression_engine = ExpressionEngine::ExtendScript;
    let bakes = bake(&project, Some(5));
    let bake = bake_of(&bakes, &project, "Title", "ADBE Rotate Z");
    assert_eq!(bake.dialect, Dialect::Legacy);
    assert_eq!(bake.repairs.len(), 1);
    let (_, channels) = numbers(bake);
    assert_eq!((channels[0][10], channels[0][11]), (0.0, 90.0));
}

#[test]
fn an_expression_over_a_2d_layers_scale_loops_over_its_two_components() {
    // A common rig: undo the parents' scale by walking up and dividing each
    // component. The parent's scale expression gives two components, and a
    // 2D layer's own scale reads in two, so no component is left undefined.
    let size = layer(
        1,
        "Size",
        vec![transform(vec![expressed(
            "ADBE Scale",
            Stream::Static(Value::Vector3([1.0, 1.0, 1.0])),
            "[200, 50]",
        )])],
    );
    let child = Layer {
        parent: Some(1),
        ..layer(
            2,
            "Child",
            vec![transform(vec![expressed(
                "ADBE Scale",
                Stream::Static(Value::Vector3([1.0, 1.0, 1.0])),
                "P = thisLayer; s = transform.scale.value; \
                 while (P.hasParent) { P = P.parent; \
                 for (i = 0; i < s.length; i++) s[i] *= 100 / P.transform.scale.value[i] } s",
            )])],
        )
    };
    let project = project(vec![(5, "Main", comp(vec![size, child], 2.0))]);
    let bakes = bake(&project, Some(5));
    let (_, channels) = numbers(bake_of(&bakes, &project, "Child", "ADBE Scale"));
    assert_eq!(
        [channels[0][0], channels[1][0], channels[2][0]],
        [0.5, 2.0, 1.0],
        "50% and 200%, and the depth it does not give stays its own"
    );
}

#[test]
fn an_expression_reads_a_layers_markers() {
    use crate::model::{Interpolation, Keyframe, Marker};
    let marker = |time: f64, comment: &str, duration: f64| Keyframe {
        time,
        value: Value::Marker(Box::new(Marker {
            comment: comment.into(),
            duration,
            ..Marker::default()
        })),
        in_interpolation: Interpolation::Hold,
        out_interpolation: Interpolation::Hold,
        in_ease: vec![],
        out_ease: vec![],
        spatial: None,
        continuous: false,
        auto_bezier: false,
        roving: false,
    };
    let markers = property(
        "ADBE Marker",
        Stream::Animated(vec![marker(0.5, "In", 0.0), marker(1.5, "Out", 0.25)]),
    );
    // A duration-control rig's reads: the count, a time by index, a marker
    // by its comment and its duration.
    let title = layer(
        2,
        "Title",
        vec![
            Child::Property(markers),
            transform(vec![expressed(
                "ADBE Rotate Z",
                number(0.0),
                "var m = thisLayer.marker; m.numKeys * 100 + m.key(2).time * 10 + m.key(\"Out\").duration",
            )]),
        ],
    );
    let project = project(vec![(5, "Main", comp(vec![title], 2.0))]);
    let bakes = bake(&project, Some(5));
    let (_, channels) = numbers(bake_of(&bakes, &project, "Title", "ADBE Rotate Z"));
    assert!(
        (channels[0][0] - 215.25).abs() < 1e-9,
        "2 markers, the second at 1.5 s, the one named Out 0.25 s long: {}",
        channels[0][0]
    );
}

/// A text layer `ab c` whose expression selector, counting `based_on` when
/// set, gives each part `source`.
fn selected(source: &str, based_on: Option<f64>) -> Project {
    use crate::model::{Child, Value};
    use crate::test_support::build::group;
    use crate::test_support::sample_document;
    let document = crate::model::TextDocument {
        text: "ab c".into(),
        ..sample_document()
    };
    let mut selector = vec![Child::Property(expressed(
        "ADBE Text Expressible Amount",
        Stream::Static(Value::Vector3([100.0, 100.0, 100.0])),
        source,
    ))];
    if let Some(n) = based_on {
        selector.push(Child::Property(property(
            "ADBE Text Range Type2",
            number(n),
        )));
    }
    let animator = group(
        "ADBE Text Animator",
        vec![Child::Group(group(
            "ADBE Text Selectors",
            vec![Child::Group(group(
                "ADBE Text Expressible Selector",
                selector,
            ))],
        ))],
    );
    let text = group(
        "ADBE Text Properties",
        vec![
            Child::Property(property(
                "ADBE Text Document",
                Stream::Static(Value::Text(Box::new(document))),
            )),
            Child::Group(group("ADBE Text Animators", vec![Child::Group(animator)])),
        ],
    );
    let title = layer(1, "Title", vec![Child::Group(text)]);
    project(vec![(5, "Main", comp(vec![title], 1.0))])
}

/// The amount of the expression selector of `project`.
fn amount(project: &Project) -> &Property {
    project
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Composition(comp) => Some(comp),
            _ => None,
        })
        .flat_map(|comp| &comp.layers)
        .flat_map(|l| l.properties.properties())
        .find(|p| p.match_name == "ADBE Text Expressible Amount")
        .expect("the amount")
}

#[test]
fn an_expression_selector_is_baked_once_per_part() {
    let project = selected("[100 * textIndex / textTotal, 0, 0]", None);
    let bakes = bake(&project, Some(5));
    let parts = bakes
        .parts(amount(&project))
        .expect("baked")
        .as_ref()
        .unwrap();
    // `ab c` has four characters, the space among them.
    let firsts: Vec<f64> = parts
        .iter()
        .map(|samples| match &samples.values {
            Sampled::Numbers { channels, .. } => channels[0][0],
            other => panic!("not numbers: {other:?}"),
        })
        .collect();
    assert_eq!(firsts, [25.0, 50.0, 75.0, 100.0]);
    assert!(parts.iter().all(|samples| samples.constant));
    assert!(
        bakes.get(amount(&project)).is_none(),
        "not baked as one value"
    );
}

/// Title, a text layer, and Box, a shape layer whose rectangle's size reads
/// Title's rectangle at each frame and whose anchor reads it at 1 s, padded
/// by its offset's amount; and Plate, a layer over a 640 by 360 solid.
fn boxed() -> Project {
    use crate::model::LayerKind;
    let title = Layer {
        kind: LayerKind::Text,
        ..layer(2, "Title", vec![transform(vec![])])
    };
    let offset = Group {
        name: Some("Offset Paths 1".into()),
        ..crate::test_support::build::group(
            "ADBE Vector Filter - Offset",
            vec![Child::Property(property(
                "ADBE Vector Offset Amount",
                number(8.0),
            ))],
        )
    };
    let rect = crate::test_support::build::group(
        "ADBE Vector Shape - Rect",
        vec![Child::Property(expressed(
            "ADBE Vector Rect Size",
            point(0.0, 0.0),
            r#"var r = thisComp.layer("Title").sourceRectAtTime(); [r.width, r.height]"#,
        ))],
    );
    let contents = crate::test_support::build::group(
        "ADBE Root Vectors Group",
        vec![Child::Group(rect), Child::Group(offset)],
    );
    let shapes = Layer {
        kind: LayerKind::Shape,
        ..layer(
            3,
            "Box",
            vec![
                Child::Group(contents),
                transform(vec![expressed(
                    "ADBE Anchor Point",
                    point(0.0, 0.0),
                    r#"var r = thisComp.layer("Title").sourceRectAtTime(1, true); var o = content("Offset Paths 1").amount; [r.left - o, r.top - o]"#,
                )]),
            ],
        )
    };
    let plate = Layer {
        source: Some(9),
        ..layer(
            4,
            "Plate",
            vec![transform(vec![expressed(
                "ADBE Rotate Z",
                number(0.0),
                "sourceRectAtTime().width + sourceRectAtTime(0, true).height",
            )])],
        )
    };
    let mut project = project(vec![(5, "Main", comp(vec![title, shapes, plate], 0.3))]);
    project
        .items
        .push(crate::test_support::build::solid(9, 640, 360));
    project
}

#[test]
fn a_rectangle_not_measured_is_asked_for_at_every_frame_that_reads_it() {
    let project = boxed();
    let bakes = bake(&project, Some(5));
    let size = bake_of(&bakes, &project, "Box", "ADBE Vector Rect Size");
    assert_eq!(size.outcome, Err(crate::bake::UNMEASURED.into()));
    let asked: Vec<(u32, f64, bool)> = bakes
        .asks
        .iter()
        .map(|a| (a.layer, a.time, a.extents))
        .collect();
    // Frames 0, 0.1 and 0.2 s for the size; 1 s with extents for the
    // anchor, once though it runs at every frame.
    assert_eq!(
        asked,
        [
            (2, 0.0, false),
            (2, 0.1, false),
            (2, 0.2, false),
            (2, 1.0, true)
        ]
    );
    // The size is part of what Box draws; the anchor is not, and Title's
    // content holds no expression.
    assert_eq!(bakes.unsettled.iter().collect::<Vec<_>>(), [&(5, 3)]);
    // A layer with a source covers its source, at any time.
    let (_, plate) = numbers(bake_of(&bakes, &project, "Plate", "ADBE Rotate Z"));
    assert_eq!(plate[0], [1000.0, 1000.0, 1000.0]);
}

#[test]
fn measured_rectangles_are_read_as_left_top_width_height() {
    let project = boxed();
    let mut layouts = Layouts::default();
    for ask in &bake(&project, Some(5)).asks {
        let grow = (ask.time * 10.0).round();
        layouts.insert(ask, Ok([-5.0, -30.0, 100.0 + grow, 40.0]));
    }
    let bakes = bake_with(&project, Some(5), &layouts);
    assert!(bakes.asks.is_empty() && bakes.unsettled.is_empty());
    let (_, size) = numbers(bake_of(&bakes, &project, "Box", "ADBE Vector Rect Size"));
    assert_eq!(
        size[..2],
        [vec![100.0, 101.0, 102.0], vec![40.0, 40.0, 40.0]]
    );
    let (_, anchor) = numbers(bake_of(&bakes, &project, "Box", "ADBE Anchor Point"));
    assert_eq!(
        [anchor[0][0], anchor[1][0]],
        [-13.0, -38.0],
        "less the offset's amount"
    );
}

#[test]
fn an_expression_selector_counts_by_its_based_on() {
    let project = selected("time * 100 + textIndex", Some(3.0));
    let bakes = bake(&project, Some(5));
    let parts = bakes
        .parts(amount(&project))
        .expect("baked")
        .as_ref()
        .unwrap();
    assert_eq!(parts.len(), 2, "two words");
    assert!(!parts[0].constant, "it reads the time");
    assert_eq!(parts[1].times.len(), 10);
}

#[test]
fn a_rectangle_that_could_not_be_measured_says_why() {
    let project = boxed();
    let mut layouts = Layouts::default();
    for ask in &bake(&project, Some(5)).asks {
        layouts.insert(ask, Err("the scene leaves the layer out".into()));
    }
    let bakes = bake_with(&project, Some(5), &layouts);
    let size = bake_of(&bakes, &project, "Box", "ADBE Vector Rect Size");
    assert_eq!(
        size.outcome,
        Err(
            "it reads the size of a layer's content, which could not be measured (the scene \
             leaves the layer out), which the import cannot know"
                .into()
        )
    );
    assert!(bakes.asks.is_empty() && bakes.unsettled.is_empty());
}
