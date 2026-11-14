//! Unit tests for `src/bake/mod.rs`: evaluating a project's expressions.

use pretty_assertions::assert_eq;

use super::*;
use crate::model::{Child, Layer};
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
