//! Unit tests for `src/map/expressions.rs`: baked expressions as scene keys.

use pretty_assertions::assert_eq;
use serde_json::{Value as Json, json};

use crate::map::{Import, Options, Outcome, import};
use crate::model::{Stream, Value};
use crate::test_support::build::{comp, expressed, layer, number, point, project, transform};

/// Main, 2 seconds at 10 frames a second, holding one layer `Title` whose
/// transform has `properties`.
fn imported(properties: Vec<crate::model::Property>) -> Import {
    let title = layer(1, "Title", vec![transform(properties)]);
    import(
        &project(vec![(5, "Main", comp(vec![title], 2.0))]),
        &Options::default(),
    )
    .expect("it imports")
}

fn title(import: &Import) -> &Json {
    &import.json["layers"][0]
}

#[test]
fn a_moving_position_is_written_one_component_at_a_time() {
    let import = imported(vec![expressed(
        "ADBE Position",
        point(10.0, 20.0),
        "value + [time * 100, 0]",
    )]);
    let layer = title(&import);
    let x = &layer["anim"]["x"];
    assert_eq!(
        x.as_array().map(Vec::len),
        Some(2),
        "a straight line is two keys: {x}"
    );
    assert_eq!(x[0]["v"], json!(10));
    assert_eq!(x[0]["curve"], json!("speed(100,0.333333,100,0.333333)"));
    assert_eq!(x[1]["t"], json!("1.9s"));
    assert!(layer["anim"].get("y").is_none(), "y never moves: {layer}");
    assert_eq!(layer["position"], json!([10, 20]));
    let report = &import.mapping.expressions[0];
    assert_eq!(report.engine, "modern");
    assert_eq!(report.frames, 20);
    assert!(
        matches!(
            &report.outcome,
            Outcome::Baked {
                keys: 2,
                keys_before: 0,
                approximate: false,
                ..
            }
        ),
        "{report:?}"
    );
}

#[test]
fn a_value_that_never_changes_is_written_still() {
    let import = imported(vec![expressed("ADBE Opacity", number(1.0), "value / 4")]);
    let layer = title(&import);
    assert_eq!(layer["opacity"], json!(0.25));
    assert!(layer.get("anim").is_none(), "{layer}");
    assert!(matches!(
        import.mapping.expressions[0].outcome,
        Outcome::Baked { keys: 0, .. }
    ));
}

#[test]
fn components_of_a_value_share_their_keys_with_a_curve_each() {
    let import = imported(vec![expressed(
        "ADBE Scale",
        Stream::Static(Value::Vector3([1.0, 1.0, 1.0])),
        "[100 + time * 50, 100 - time * 10, 100]",
    )]);
    let scale = &title(&import)["anim"]["scale"];
    assert_eq!(scale[0]["v"], json!([1, 1]));
    assert_eq!(
        scale[0]["curve"],
        json!([
            "speed(0.5,0.333333,0.5,0.333333)",
            "speed(-0.1,0.333333,-0.1,0.333333)"
        ])
    );
}

#[test]
fn an_expression_that_fails_keeps_its_keys_and_says_why() {
    let import = imported(vec![expressed(
        "ADBE Rotate Z",
        number(30.0),
        r#"thisComp.layer("Nowhere").transform.rotation"#,
    )]);
    assert_eq!(title(&import)["rotate"], json!(30));
    let Outcome::NotBaked { why } = &import.mapping.expressions[0].outcome else {
        panic!("not baked");
    };
    assert!(why.contains("Nowhere"), "{why}");
    assert!(
        import
            .mapping
            .unsupported
            .iter()
            .any(|f| f.what.starts_with("an expression it could not bake")),
        "{:?}",
        import.mapping.unsupported
    );
    assert!(
        import.mapping.to_string().contains(
            "expressions: 0 baked (0 approximate) into 0 keys, where 0 were; 1 not baked"
        )
    );
}

#[test]
fn controls_and_whole_sources_are_kept_in_the_report() {
    use crate::test_support::build::{property, slider};
    let controls = layer(
        2,
        "CONTROLS",
        vec![slider(
            "Speed",
            Some(property("ADBE Slider Control-0001", number(4.0))),
        )],
    );
    let title = layer(
        1,
        "Title",
        vec![transform(vec![expressed(
            "ADBE Rotate Z",
            number(0.0),
            "// spin\nthisComp.layer(\"CONTROLS\").effect(\"Speed\")(\"Slider\") * time",
        )])],
    );
    let import = import(
        &project(vec![(5, "Main", comp(vec![title, controls], 2.0))]),
        &Options::default(),
    )
    .expect("it imports");
    let control = &import.mapping.controls[0];
    assert_eq!(control.name, "Speed");
    assert_eq!(control.kind, "ADBE Slider Control");
    assert_eq!(control.values["Slider"], json!({ "number": 4.0 }));
    assert!(
        control
            .at
            .ends_with("CONTROLS/ADBE Effect Parade/ADBE Slider Control"),
        "{}",
        control.at
    );
    assert_eq!(
        import.mapping.expressions[0].source,
        "// spin\nthisComp.layer(\"CONTROLS\").effect(\"Speed\")(\"Slider\") * time"
    );
}

#[test]
fn what_an_expression_gives_outside_the_range_is_clamped_frame_by_frame() {
    // 50% plus 100% a second passes 100% after half a second.
    let import = imported(vec![expressed(
        "ADBE Opacity",
        number(0.5),
        "value + time * 100",
    )]);
    let keys = title(&import)["anim"]["opacity"]
        .as_array()
        .expect("a track")
        .clone();
    assert!(
        keys.iter()
            .all(|k| k["v"].as_f64().is_some_and(|v| v <= 1.0)),
        "{keys:?}"
    );
    assert_eq!(keys.last().map(|k| k["v"].clone()), Some(json!(1)));
    assert_eq!(import.mapping.clamped.len(), 1);
    assert_eq!(
        import.mapping.clamped[0].what,
        "14 frames of 20 an expression gives outside 0..=1"
    );
}

#[test]
fn a_constant_outside_the_range_is_clamped_to_a_still_value() {
    let import = imported(vec![expressed("ADBE Opacity", number(0.5), "-5")]);
    assert_eq!(title(&import)["opacity"], json!(0));
    assert!(title(&import)["anim"].get("opacity").is_none());
    assert_eq!(
        import.mapping.clamped[0].what,
        "20 frames of 20 an expression gives outside 0..=1"
    );
}

#[test]
fn a_colour_an_expression_gives_is_opaque_whatever_its_alpha() {
    // A fill whose colour an expression takes from a control that stores
    // alpha 0: the original draws it opaque.
    use crate::model::{Child, Color};
    use crate::test_support::build::group;
    let stored = Stream::Static(Value::Color(Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    }));
    let fill = Child::Group(group(
        "ADBE Effect Parade",
        vec![Child::Group(group(
            "ADBE Fill",
            vec![Child::Property(expressed(
                "ADBE Fill-0002",
                stored,
                "[0.25, 0.5, 0.75, 0]",
            ))],
        ))],
    ));
    let filled = layer(1, "Title", vec![fill]);
    let import = import(
        &project(vec![(5, "Main", comp(vec![filled], 2.0))]),
        &Options::default(),
    )
    .expect("it imports");
    let effect = &title(&import)["fx"][0];
    assert_eq!(effect["fill"], json!([0.25, 0.5, 0.75, 1]), "{effect}");
}

/// A text layer `Title` in Main (2 s at 10 frames a second) whose Source
/// Text holds `document` under `source`, or keys of documents.
fn text_layer(value: Stream, source: Option<&str>) -> Import {
    use crate::model::{Child, LayerKind};
    use crate::test_support::build::{group, property};
    let document = match source {
        Some(source) => expressed("ADBE Text Document", value, source),
        None => property("ADBE Text Document", value),
    };
    let title = crate::model::Layer {
        kind: LayerKind::Text,
        ..layer(
            1,
            "Title",
            vec![Child::Group(group(
                "ADBE Text Properties",
                vec![Child::Property(document)],
            ))],
        )
    };
    import(
        &project(vec![(5, "Main", comp(vec![title], 2.0))]),
        &Options::default(),
    )
    .expect("it imports")
}

/// Words a source text expression changes become held keys of the layer's
/// words where they change, over the property's own document ([D92]), and
/// nothing is reported unsupported.
#[test]
fn source_text_an_expression_changes_becomes_keyed_words() {
    use crate::test_support::sample_document;
    let own = Stream::Static(Value::Text(Box::new(sample_document())));
    let import = text_layer(
        own,
        Some("['3', '2', '1'][Math.min(2, Math.floor(time * 2))]"),
    );
    let layer = title(&import);
    assert_eq!(layer["text"], json!(sample_document().text));
    assert_eq!(
        layer["anim"]["text"],
        json!([["0s", "3"], ["0.5s", "2"], ["1s", "1"]])
    );
    assert!(
        !import
            .mapping
            .unsupported
            .iter()
            .any(|u| u.what.contains("source text")),
        "{:?}",
        import.mapping.unsupported
    );
    assert!(
        matches!(
            &import.mapping.expressions[0].outcome,
            Outcome::Baked { keys: 3, .. }
        ),
        "{:?}",
        import.mapping.expressions[0]
    );
    // Text that never changes is the layer's words, with no keys.
    let still = text_layer(
        Stream::Static(Value::Text(Box::new(sample_document()))),
        Some("'Go'"),
    );
    // The words become a text slot (D77), which holds them.
    let resolved = nakkash_format::slots::resolved(&still.json).expect("it resolves");
    assert_eq!(resolved["layers"][0]["text"], json!("Go"));
    assert!(
        title(&still)
            .get("anim")
            .is_none_or(|a| a.get("text").is_none())
    );
}

/// A keyed Source Text becomes held keys at the times the project keeps,
/// its first key's document the layer's words and style; a key that
/// changes the style is reported, its words kept.
#[test]
fn keyed_source_text_becomes_keyed_words() {
    use crate::model::{Interpolation, Keyframe};
    use crate::test_support::sample_document;
    let key = |time: f64, words: &str, size: f64| {
        let mut document = sample_document();
        document.text = words.into();
        for run in &mut document.characters {
            run.range = 0..words.len();
            run.style.size = size;
        }
        for run in &mut document.paragraphs {
            run.range = 0..words.len();
        }
        Keyframe {
            time,
            value: Value::Text(Box::new(document)),
            in_interpolation: Interpolation::Hold,
            out_interpolation: Interpolation::Hold,
            in_ease: vec![],
            out_ease: vec![],
            spatial: None,
            continuous: false,
            auto_bezier: false,
            roving: false,
        }
    };
    let import = text_layer(
        Stream::Animated(vec![key(0.0, "Ready", 44.0), key(1.0, "Set\rGo", 44.0)]),
        None,
    );
    let layer = title(&import);
    assert_eq!(layer["text"], json!("Ready"));
    assert_eq!(
        layer["anim"]["text"],
        json!([["0s", "Ready"], ["1s", "Set\nGo"]])
    );
    assert!(
        !import
            .mapping
            .unsupported
            .iter()
            .any(|u| u.what.contains("source text")),
        "{:?}",
        import.mapping.unsupported
    );
    let restyled = text_layer(
        Stream::Animated(vec![key(0.0, "Ready", 44.0), key(1.0, "Go", 60.0)]),
        None,
    );
    let reported: Vec<&str> = restyled
        .mapping
        .unsupported
        .iter()
        .map(|u| u.what.as_str())
        .collect();
    assert!(
        reported
            .iter()
            .any(|w| w.starts_with("keyed source text whose keys change its style: 1 key")),
        "{reported:?}"
    );
}
