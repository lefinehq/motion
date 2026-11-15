//! Unit tests for `src/validate.rs`: the rules beyond the JSON shape, and
//! the defaults.

use crate::address::ID_RULE;
use crate::parse_document;
use crate::shorthand::{Coord, Position};
use crate::units::{Color, Size, Time};
use crate::{Content, Document, ImageContent};
use crate::{Diagnostic, Kind};

fn doc(json: &str) -> Document {
    parse_document(json).unwrap()
}

fn err(json: &str) -> Diagnostic {
    parse_document(json).unwrap_err()
}

/// The kind and path of the error of `json`.
fn fails(json: &str) -> (Kind, Option<String>) {
    let e = err(json);
    (e.kind, e.path_text())
}

/// `(kind, Some(path))`, to compare with [`fails`].
fn at(kind: Kind, path: &str) -> (Kind, Option<String>) {
    (kind, Some(path.to_owned()))
}

/// A scene whose layers are `layers`.
fn scene(layers: &str) -> String {
    format!(r#"{{"motion":1,"dur":"1s","layers":[{layers}]}}"#)
}

#[test]
fn parses_minimal_scene_with_defaults() {
    let d = doc(r#"{"motion":1,"dur":"2s","layers":[{"id":"a","rect":"10x10"}]}"#);
    assert_eq!(d.scene().root.fps, 30.0);
    assert_eq!(
        d.scene().root.size,
        Size {
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(
        d.scene().root.background,
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(d.scene().root.layers[0].fill, [1.0; 4]);
    assert_eq!(
        d.scene().root.layers[0].at,
        Position {
            x: Coord::Center,
            y: Coord::Center
        }
    );
    assert!(d.scene().root.layers[0].enter.is_none());
}

#[test]
fn text_layer_gets_the_default_font() {
    let d = doc(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":"Hi"}]}"#);
    match &d.scene().root.layers[0].content {
        Content::Text(text) => {
            assert_eq!(text.text, "Hi");
            let font = &text.first_style().font;
            assert_eq!(font.family, "Inter Variable");
            assert_eq!(font.size, 48.0);
            assert_eq!(font.weight, 400);
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn size_fps_and_bg_overrides_reach_the_scene() {
    let d = doc(r##"{"motion":1,"size":"720p","fps":60,"dur":"1s","bg":"#102030","layers":[]}"##);
    assert_eq!(
        d.scene().root.size,
        Size {
            width: 1280,
            height: 720
        }
    );
    assert_eq!(d.scene().root.fps, 60.0);
    assert_eq!(
        d.scene().root.background,
        Color {
            r: 0x10,
            g: 0x20,
            b: 0x30,
            a: 255
        }
    );
}

#[test]
fn version_must_be_one() {
    let e = err(r#"{"motion":2,"dur":"1s","layers":[]}"#);
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::Invalid, Some("motion"))
    );
    assert_eq!(e.expected.as_deref(), Some("`1`"));
}

#[test]
fn fps_tells_a_wrong_type_from_a_bound() {
    let fps = |v: &str| {
        fails(&format!(
            r#"{{"motion":1,"dur":"1s","fps":{v},"layers":[]}}"#
        ))
    };
    assert_eq!(fps(r#""x""#), at(Kind::WrongType, "fps"));
    assert_eq!(fps("true"), at(Kind::WrongType, "fps"));
    assert_eq!(fps("0"), at(Kind::OutOfRange, "fps"));
    assert_eq!(fps("-1"), at(Kind::OutOfRange, "fps"));
}

#[test]
fn shorthand_failures_keep_their_path_and_expected_form() {
    let e = err(r#"{"motion":1,"dur":"soon","layers":[]}"#);
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::Syntax, Some("dur"))
    );
    assert_eq!(
        e.expected,
        "soon".parse::<Time>().unwrap_err().expected,
        "the shorthand's own form"
    );
    assert_eq!(
        fails(r#"{"motion":1,"dur":"1s","size":"huge","layers":[]}"#),
        at(Kind::UnknownName, "size")
    );
    assert_eq!(
        fails(&scene(r#"{"id":"a","rect":"1x1","fill":"red"}"#)),
        at(Kind::Syntax, "a.fill")
    );
}

#[test]
fn a_layer_draws_exactly_one_content() {
    let neither = err(&scene(r#"{"id":"a"}"#));
    let both = err(&scene(r#"{"id":"a","rect":"1x1","text":"x"}"#));
    assert_eq!((neither.kind, both.kind), (Kind::Missing, Kind::Conflict));
    // The fix is the same, so the message and the form are too.
    assert_eq!(both.message, neither.message);
    assert_eq!(both.expected, neither.expected);
    // The second content key is the one to remove.
    assert_eq!(both.path_text().as_deref(), Some("a.rect"));
    assert_eq!(neither.path_text().as_deref(), Some("a"));
}

#[test]
fn font_only_applies_to_text() {
    assert_eq!(
        fails(&scene(r#"{"id":"a","rect":"1x1","font":"Inter 9"}"#)),
        at(Kind::Conflict, "a.font")
    );
}

#[test]
fn an_image_layer_names_its_file_and_fills_its_box() {
    let d = doc(&scene(
        r#"{"id":"a","image":"photos/beach.jpg","box":[192,108]},{"id":"b","image":"/abs/b.png","box":[-8,-4,16,8]}"#,
    ));
    let image = |i: usize| match &d.scene().root.layers[i].content {
        Content::Image(image) => image.clone(),
        other => panic!("not an image: {other:?}"),
    };
    assert_eq!(
        image(0),
        ImageContent {
            path: "photos/beach.jpg".into(),
            frame: kurbo::Rect::new(0.0, 0.0, 192.0, 108.0),
        }
    );
    assert_eq!(image(1).path, "/abs/b.png");
    assert_eq!(image(1).frame, kurbo::Rect::new(-8.0, -4.0, 8.0, 4.0));
}

#[test]
fn an_image_needs_a_path_and_a_box() {
    let missing = err(&scene(r#"{"id":"a","image":"a.png"}"#));
    assert_eq!(
        (missing.kind, missing.path_text().as_deref()),
        (Kind::Missing, Some("a"))
    );
    assert_eq!(
        missing.expected.as_deref(),
        Some(r#"the picture's size in px, e.g. "box":[1920,1080]"#)
    );
    let empty = err(&scene(r#"{"id":"a","image":" ","box":[1,1]}"#));
    assert_eq!(
        (empty.kind, empty.path_text().as_deref()),
        (Kind::Invalid, Some("a.image"))
    );
    assert!(empty.expected.is_some());
    assert_eq!(
        fails(&scene(r#"{"id":"a","image":"a.png","box":[0,1]}"#)),
        at(Kind::Invalid, "a.box")
    );
    assert_eq!(
        fails(&scene(r#"{"id":"a","image":3,"box":[1,1]}"#)),
        at(Kind::WrongType, "a.image")
    );
}

#[test]
fn box_applies_to_text_and_images_only_and_an_image_is_its_only_content() {
    assert_eq!(
        fails(&scene(r#"{"id":"a","rect":"1x1","box":[1,1]}"#)),
        at(Kind::Conflict, "a.box")
    );
    assert_eq!(
        fails(&scene(
            r#"{"id":"a","rect":"1x1","image":"a.png","box":[1,1]}"#
        )),
        at(Kind::Conflict, "a.image")
    );
    assert_eq!(
        fails(&scene(
            r#"{"id":"a","image":"a.png","box":[1,1],"font":"Inter 9"}"#
        )),
        at(Kind::Conflict, "a.font")
    );
}

#[test]
fn ids_follow_the_id_rule_and_are_unique() {
    let e = err(&scene(r#"{"id":"Bad Id","rect":"1x1"}"#));
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::Invalid, Some("layers[0].id"))
    );
    assert_eq!(e.expected.as_deref(), Some(ID_RULE));
    let e = err(&scene(r#"{"id":"scene","rect":"1x1"}"#));
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::Conflict, Some("layers[0].id"))
    );
    assert!(e.expected.is_some());
    assert_eq!(
        fails(&scene(r#"{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}"#)),
        at(Kind::Conflict, "layers[1].id")
    );
}

#[test]
fn an_entrance_names_a_preset_that_exists() {
    let e = err(&scene(
        r#"{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10","in":"zoom"}"#,
    ));
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::UnknownName, Some("b.in"))
    );
    assert!(e.expected.is_some_and(|names| names.contains("fade-up")));
}

#[test]
fn a_value_that_is_not_finite_is_not_a_number() {
    assert_eq!(
        fails(&scene(
            r#"{"id":"a","rect":"1x1","anim":{"x":"inf -> 1 @0-5f"}}"#,
        )),
        at(Kind::Syntax, "a.anim.x")
    );
}

#[test]
fn position_sets_an_exact_point_whatever_the_parent_box() {
    let d = doc(&scene(r#"{"id":"a","rect":"10x10","position":[-3.5,12]}"#));
    let layer = &d.scene().root.layers[0];
    use crate::Value;
    use crate::property::Prop;
    let shifted = kurbo::Rect::new(-40.0, -30.0, 60.0, 70.0);
    assert_eq!(
        layer.rest(
            Prop::POSITION,
            shifted,
            kurbo::Rect::new(0.0, 0.0, 10.0, 10.0)
        ),
        Value::Point([-3.5, 12.0])
    );
}

#[test]
fn property_keys_set_resting_values_of_their_type() {
    let d = doc(&scene(
        r#"{"id":"a","rect":"10x10","rotate":45,"scale":0.8,"anchor":[0,0],"opacity":"0.5"}"#,
    ));
    let layer = &d.scene().root.layers[0];
    use crate::Value;
    use crate::property::Prop;
    assert_eq!(layer.value(Prop::ROTATE), Some(Value::Number(45.0)));
    // One number scales both axes.
    assert_eq!(layer.value(Prop::SCALE), Some(Value::Point([0.8, 0.8])));
    assert_eq!(layer.value(Prop::OPACITY), Some(Value::Number(0.5)));
    assert_eq!(layer.value(Prop::POSITION), None);
    assert_eq!(
        layer.rest(
            Prop::ANCHOR,
            kurbo::Rect::new(0.0, 0.0, 100.0, 100.0),
            kurbo::Rect::new(0.0, 0.0, 10.0, 10.0)
        ),
        Value::Point([0.0, 0.0])
    );
    assert_eq!(
        layer.rest(
            Prop::POSITION,
            kurbo::Rect::new(0.0, 0.0, 100.0, 100.0),
            kurbo::Rect::new(0.0, 0.0, 10.0, 10.0)
        ),
        Value::Point([50.0, 50.0])
    );
}

#[test]
fn a_mixed_list_is_not_a_point() {
    // A list holds numbers only; a string inside it is a wrong type.
    assert_eq!(
        fails(&scene(r#"{"id":"a","rect":"10x10","anchor":[0,"x"]}"#)).0,
        Kind::WrongType
    );
}

#[test]
fn timing_and_parent_default_and_parse() {
    let d = doc(&scene(
        r#"{"id":"a","rect":"10x10"},{"id":"b","rect":"5x5","parent":"a","span":"0.5s-2s","offset":"12f","stretch":-1}"#,
    ));
    let [a, b] = &d.scene().root.layers[..] else {
        panic!("two layers");
    };
    assert_eq!(
        (a.parent.as_deref(), a.span, a.offset, a.stretch),
        (None, None, Time::Frames(0.0), 1.0)
    );
    assert_eq!(b.parent.as_deref(), Some("a"));
    let span = b.span.unwrap();
    assert_eq!(
        (span.start, span.end),
        (Time::Seconds(0.5), Time::Seconds(2.0))
    );
    assert_eq!((b.offset, b.stretch), (Time::Frames(12.0), -1.0));
}

#[test]
fn a_parent_chain_that_ends_is_fine_and_a_loop_is_not() {
    assert!(parse_document(&scene(
        r#"{"id":"a","rect":"1x1","parent":"b"},{"id":"b","rect":"1x1","parent":"c"},{"id":"c","rect":"1x1"}"#
    ))
    .is_ok());
    assert_eq!(
        fails(&scene(
            r#"{"id":"a","rect":"1x1","parent":"b"},{"id":"b","rect":"1x1","parent":"c"},{"id":"c","rect":"1x1","parent":"b"}"#
        )),
        at(Kind::Invalid, "b.parent")
    );
}

#[test]
fn a_matte_names_a_layer_of_its_list_and_a_mode() {
    use crate::shorthand::{MATTE_MODES, Matte, MatteMode};
    let d = doc(&scene(
        r#"{"id":"a","rect":"1x1","matte":"b"},{"id":"b","rect":"1x1","matte":" c  luma-inverted "},{"id":"c","rect":"1x1","hidden":true}"#,
    ));
    let mattes: Vec<Option<Matte>> = d
        .scene()
        .root
        .layers
        .iter()
        .map(|l| l.matte.clone())
        .collect();
    let matte = |layer: &str, mode| {
        Some(Matte {
            layer: layer.into(),
            mode,
        })
    };
    assert_eq!(
        mattes,
        [
            matte("b", MatteMode::Alpha),
            matte("c", MatteMode::LumaInverted),
            None
        ]
    );
    for (name, mode) in MATTE_MODES {
        assert_eq!(mode.name(), name);
        assert_eq!(format!("x {name}").parse::<Matte>().unwrap().mode, mode);
    }
    assert_eq!(
        fails(&scene(
            r#"{"id":"a","rect":"1x1","matte":"b"},{"id":"b","rect":"1x1","matte":"c"},{"id":"c","rect":"1x1","matte":"b"}"#
        )),
        at(Kind::Invalid, "b.matte")
    );
}

#[test]
fn object_keys_carry_a_path_and_curves_per_component() {
    use crate::anim::Easing;
    use crate::{Curve, Value};
    let d = doc(&scene(
        r#"{"id":"a","rect":"10x10","anim":{
        "position":[{"t":0,"v":[0,0],"curve":"speed(0,0.5,0,0.5)","path":[[10,0],[0,-10]]},{"t":"1s","v":"100,100"}],
        "scale":[[0,1,["ease-in","hold"]],[10,[2,3]]]}}"#,
    ));
    let anim = &d.scene().root.layers[0].anim;
    let position = &anim[0].keys;
    assert_eq!(position[0].path, Some([[10.0, 0.0], [0.0, -10.0]]));
    assert_eq!(position[1].value, Value::Point([100.0, 100.0]));
    assert!(matches!(
        position[0].easing,
        Easing::All(Curve::Speed { .. })
    ));
    let scale = &anim[1].keys;
    assert_eq!(scale[0].value, Value::Point([1.0, 1.0]));
    assert_eq!(
        scale[0].easing,
        Easing::Each(vec![Curve::named("ease-in").unwrap(), Curve::Hold])
    );
    assert_eq!(scale[0].easing.of(1), Curve::Hold);
}

#[test]
fn a_preset_may_scale_but_not_place_with_one_number() {
    assert!(
        parse_document(r#"{"motion":1,"dur":"1s","presets":{"grow":{"scale":0.2}},"layers":[]}"#)
            .is_ok()
    );
    let e = err(r#"{"motion":1,"dur":"1s","presets":{"jump":{"position":5}},"layers":[]}"#);
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::WrongType, Some("presets.jump.position"))
    );
    let e = err(r#"{"motion":1,"dur":"1s","presets":{"far":{"scale":"+d"}},"layers":[]}"#);
    assert_eq!(e.kind, Kind::OutOfRange);
}

/// `label` holds `title` and `badge`; `card` holds a `label` layer. The
/// root's layers follow, with `kv` on a `label` layer called `second`.
fn instances(kv: &str) -> String {
    format!(
        r##"{{"motion":1,"dur":"1s","comps":{{
          "label":{{"size":"100x50","dur":"1s","layers":[
            {{"id":"badge","rect":"10x10","fill":"#00FF00"}},
            {{"id":"title","text":"Shared"}}]}},
          "card":{{"size":"200x100","dur":"1s","layers":[{{"id":"label","comp":"label"}}]}}}},
        "layers":[{{"id":"first","comp":"label"}},{{"id":"second","comp":"label",{kv}}},
          {{"id":"deep","comp":"card","overrides":{{"label/title":{{"text":"Deep"}}}}}}]}}"##
    )
}

/// The composition a layer draws, with its overrides set.
fn drawn(layer: &crate::Layer) -> &crate::CompContent {
    match &layer.content {
        Content::Comp(comp) => comp,
        other => panic!("not a comp layer: {other:?}"),
    }
}

/// The text of `layer`, a text layer.
fn words(layer: &crate::Layer) -> &str {
    match &layer.content {
        Content::Text(text) => &text.text,
        other => panic!("not a text layer: {other:?}"),
    }
}

#[test]
fn overrides_set_keys_over_the_inner_layer_of_one_instance() {
    let d = doc(&instances(
        r##""overrides":{"title":{"text":"Sale"},"badge":{"fill":"#FF0000"}}"##,
    ));
    let [first, second, deep] = &d.scene().root.layers[..] else {
        panic!("three layers");
    };
    // A layer without overrides draws the composition every such layer shares.
    assert_eq!(drawn(first).instance, None);
    let inner = drawn(second).instance.as_ref().expect("an instance");
    assert_eq!(words(&inner[1]), "Sale");
    assert_eq!(inner[0].fill, [1.0, 0.0, 0.0, 1.0]);
    // The composition itself does not change.
    assert_eq!(words(&d.scene().comps[0].layers[1]), "Shared");
    // Deeper addresses go on as the inner `comp` layer's own overrides.
    let label = &drawn(deep).instance.as_ref().expect("an instance")[0];
    let title = &drawn(label).instance.as_ref().expect("an inner instance")[1];
    assert_eq!(words(title), "Deep");
}

#[test]
fn an_override_error_points_at_the_inner_layer() {
    assert_eq!(
        fails(&instances(r#""overrides":{"title":{"text":5}}"#)),
        at(Kind::WrongType, "second/title.text")
    );
    let deep = instances("\"hidden\":false").replace(r#""text":"Deep""#, r#""fill":"red""#);
    assert_eq!(fails(&deep), at(Kind::Syntax, "deep/label/title.fill"));
    assert_eq!(
        fails(&instances(r#""overrides":{"ghost":{}}"#)),
        at(Kind::UnknownName, "second.overrides")
    );
}
