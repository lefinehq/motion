//! Unit tests for `src/validate.rs`: the rules beyond the JSON shape, and
//! the defaults.

use crate::parse_document;
use crate::shorthand::{Coord, Position};
use crate::units::{Color, Size};
use crate::{Content, Document};
use motion_diagnostic::{Diagnostic, Kind};

fn doc(json: &str) -> Document {
    parse_document(json).unwrap()
}

fn err(json: &str) -> Diagnostic {
    parse_document(json).unwrap_err()
}

fn msg(json: &str) -> String {
    err(json).to_string()
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
    assert_eq!(e.kind, Kind::Invalid);
    assert_eq!(e.to_string(), "motion: unsupported version 2; expected `1`");
}

#[test]
fn fps_tells_a_wrong_type_from_a_bound() {
    let fps = |v: &str| {
        err(&format!(
            r#"{{"motion":1,"dur":"1s","fps":{v},"layers":[]}}"#
        ))
    };
    assert_eq!(fps(r#""x""#).kind, Kind::WrongType);
    assert_eq!(fps("true").kind, Kind::WrongType);
    assert_eq!(fps("0").kind, Kind::OutOfRange);
    assert_eq!(fps("-1").kind, Kind::OutOfRange);
    assert_eq!(
        fps("0").to_string(),
        "fps: must be a number above zero; expected e.g. `30`"
    );
}

#[test]
fn shorthand_failures_keep_their_path_and_expected_form() {
    let e = err(r#"{"motion":1,"dur":"soon","layers":[]}"#);
    assert_eq!(e.path.as_deref(), Some("dur"));
    assert_eq!(
        e.to_string(),
        "dur: `soon` is not a valid time; expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`"
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","size":"huge","layers":[]}"#)
            .path
            .as_deref(),
        Some("size")
    );
    let e = err(&scene(r#"{"id":"a","rect":"1x1","fill":"red"}"#));
    assert_eq!(e.kind, Kind::Syntax);
    assert_eq!(e.path.as_deref(), Some("layers[0].fill"));
}

#[test]
fn a_layer_draws_exactly_one_of_text_or_rect() {
    let neither = err(&scene(r#"{"id":"a"}"#));
    assert_eq!(neither.kind, Kind::Missing);
    assert_eq!(
        neither.to_string(),
        r#"layers[0]: needs exactly one of `text` or `rect`; expected e.g. "text":"Hello" or "rect":"300x100""#
    );
    let both = err(&scene(r#"{"id":"a","rect":"1x1","text":"x"}"#));
    assert_eq!(both.kind, Kind::Conflict);
    assert_eq!(both.message, neither.message);
    assert_eq!(both.path.as_deref(), Some("layers[0]"));
}

#[test]
fn font_only_applies_to_text() {
    assert_eq!(
        msg(&scene(r#"{"id":"a","rect":"1x1","font":"Inter 9"}"#)),
        "layers[0].font: `font` only applies to text layers; expected `font` only next to `text`"
    );
}

#[test]
fn ids_follow_the_id_rule_and_are_unique() {
    assert_eq!(
        msg(&scene(r#"{"id":"Bad Id","rect":"1x1"}"#)),
        "layers[0].id: invalid id `Bad Id`; expected ids use lowercase letters, digits, `-` and `_`, starting with a letter"
    );
    let e = err(&scene(r#"{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}"#));
    assert_eq!(e.kind, Kind::Conflict);
    assert_eq!(
        e.to_string(),
        "layers[1].id: duplicate id `a`; expected every layer to have its own id"
    );
}

#[test]
fn an_entrance_names_a_preset_that_exists() {
    let e = err(&scene(
        r#"{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10","in":"zoom"}"#,
    ));
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(e.path.as_deref(), Some("layers[1].in"));
    assert!(e.expected.is_some());
    assert!(
        e.to_string()
            .starts_with("layers[1].in: unknown preset `zoom`")
    );
}

#[test]
fn a_value_that_is_not_finite_is_out_of_range() {
    let e = err(&scene(
        r#"{"id":"a","rect":"1x1","anim":{"x":"inf -> 1 @0-5f"}}"#,
    ));
    assert_eq!(e.kind, Kind::OutOfRange);
    assert_eq!(
        e.to_string(),
        "layers[0].anim.x: `inf` is not a finite number; expected a finite number"
    );
}
