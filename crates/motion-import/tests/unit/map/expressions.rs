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
