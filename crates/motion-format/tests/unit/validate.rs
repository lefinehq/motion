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
    assert_eq!(
        d.scene().layers[0].fill,
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        }
    );
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
        Content::Text { text, font } => {
            assert_eq!(text, "Hi");
            assert_eq!(font.family, "Inter");
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
fn a_layer_draws_exactly_one_of_text_or_rect() {
    let neither = err(&scene(r#"{"id":"a"}"#));
    let both = err(&scene(r#"{"id":"a","rect":"1x1","text":"x"}"#));
    assert_eq!((neither.kind, both.kind), (Kind::Missing, Kind::Conflict));
    // The fix is the same, so the message and the form are too.
    assert_eq!(both.message, neither.message);
    assert_eq!(both.expected, neither.expected);
    assert_eq!(both.path.as_deref(), Some("layers[0]"));
    assert_eq!(neither.path, both.path);
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
fn a_value_that_is_not_finite_is_out_of_range() {
    assert_eq!(
        fails(&scene(
            r#"{"id":"a","rect":"1x1","anim":{"x":"inf -> 1 @0-5f"}}"#,
        )),
        at(Kind::OutOfRange, "layers[0].anim.x")
    );
}
