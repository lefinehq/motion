//! Unit tests for `src/validate.rs`: the rules beyond the JSON shape, and
//! the defaults.

use super::ID_RULE;
use crate::parse_document;
use crate::shorthand::{Coord, Position};
use crate::units::{Color, Size, Time};
use crate::{Content, Document};
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
    (e.kind, e.path)
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
    assert_eq!(d.scene().fps, 30.0);
    assert_eq!(
        d.scene().size,
        Size {
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(
        d.scene().background,
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(d.scene().layers[0].fill, [1.0; 4]);
    assert_eq!(
        d.scene().layers[0].at,
        Position {
            x: Coord::Center,
            y: Coord::Center
        }
    );
    assert!(d.scene().layers[0].enter.is_none());
}

#[test]
fn text_layer_gets_the_default_font() {
    let d = doc(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":"Hi"}]}"#);
    match &d.scene().layers[0].content {
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
        d.scene().size,
        Size {
            width: 1280,
            height: 720
        }
    );
    assert_eq!(d.scene().fps, 60.0);
    assert_eq!(
        d.scene().background,
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
    assert_eq!((e.kind, e.path.as_deref()), (Kind::Invalid, Some("motion")));
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
    assert_eq!((e.kind, e.path.as_deref()), (Kind::Syntax, Some("dur")));
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
        at(Kind::Syntax, "layers[0].fill")
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
    assert_eq!(both.path.as_deref(), Some("layers[0].rect"));
    assert_eq!(neither.path.as_deref(), Some("layers[0]"));
}

#[test]
fn font_only_applies_to_text() {
    assert_eq!(
        fails(&scene(r#"{"id":"a","rect":"1x1","font":"Inter 9"}"#)),
        at(Kind::Conflict, "layers[0].font")
    );
}

#[test]
fn ids_follow_the_id_rule_and_are_unique() {
    let e = err(&scene(r#"{"id":"Bad Id","rect":"1x1"}"#));
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::Invalid, Some("layers[0].id"))
    );
    assert_eq!(e.expected.as_deref(), Some(ID_RULE));
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
        (e.kind, e.path.as_deref()),
        (Kind::UnknownName, Some("layers[1].in"))
    );
    assert!(e.expected.is_some_and(|names| names.contains("fade-up")));
}

#[test]
fn a_value_that_is_not_finite_is_not_a_number() {
    assert_eq!(
        fails(&scene(
            r#"{"id":"a","rect":"1x1","anim":{"x":"inf -> 1 @0-5f"}}"#,
        )),
        at(Kind::Syntax, "layers[0].anim.x")
    );
}

#[test]
fn position_sets_an_exact_point_whatever_the_parent_box() {
    let d = doc(&scene(r#"{"id":"a","rect":"10x10","position":[-3.5,12]}"#));
    let layer = &d.scene().layers[0];
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
    let layer = &d.scene().layers[0];
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
    let [a, b] = &d.scene().layers[..] else {
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
        at(Kind::Invalid, "layers[1].parent")
    );
}

#[test]
fn a_matte_names_a_layer_of_its_list_and_a_mode() {
    use crate::shorthand::{MATTE_MODES, Matte, MatteMode};
    let d = doc(&scene(
        r#"{"id":"a","rect":"1x1","matte":"b"},{"id":"b","rect":"1x1","matte":" c  luma-inverted "},{"id":"c","rect":"1x1","hidden":true}"#,
    ));
    let mattes: Vec<Option<Matte>> = d.scene().layers.iter().map(|l| l.matte.clone()).collect();
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
        at(Kind::Invalid, "layers[1].matte")
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
    let anim = &d.scene().layers[0].anim;
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
        (e.kind, e.path.as_deref()),
        (Kind::WrongType, Some("presets.jump.position"))
    );
    let e = err(r#"{"motion":1,"dur":"1s","presets":{"far":{"scale":"+d"}},"layers":[]}"#);
    assert_eq!(e.kind, Kind::OutOfRange);
}
