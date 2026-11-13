//! Unit tests for `src/parse.rs`.

use super::*;
use crate::shorthand::{Coord, Position};
use crate::units::{Color, Size};

#[test]
fn parses_minimal_scene_with_defaults() {
    let d =
        parse_document(r#"{"motion":1,"dur":"2s","layers":[{"id":"a","rect":"10x10"}]}"#).unwrap();
    assert_eq!(d.scene.fps, 30.0);
    assert_eq!(
        d.scene.size,
        Size {
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(
        d.scene.background,
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        d.scene.layers[0].fill,
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        }
    );
    assert_eq!(
        d.scene.layers[0].at,
        Position {
            x: Coord::Center,
            y: Coord::Center
        }
    );
    assert!(d.scene.layers[0].enter.is_none());
}

#[test]
fn text_layer_gets_the_default_font() {
    let d = parse_document(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":"Hi"}]}"#).unwrap();
    match &d.scene.layers[0].content {
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
fn errors_carry_a_path_and_the_expected_form() {
    let e = parse_document(
        r#"{"motion":1,"dur":"2s","layers":[{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10","in":"zoom"}]}"#,
    )
    .unwrap_err();
    assert_eq!(e.path.as_deref(), Some("layers[1].in"));
    assert!(e.expected.is_some());
    assert!(
        e.to_string()
            .starts_with("layers[1].in: unknown preset `zoom`")
    );
}

#[test]
fn rejects_structural_mistakes() {
    let err = |j: &str| parse_document(j).unwrap_err().to_string();
    assert!(err(r#"{"motion":2,"dur":"1s","layers":[]}"#).contains("unsupported version 2"));
    assert!(err(r#"{"motion":1,"layers":[]}"#).contains("scene: missing `dur`"));
    assert!(
        err(r##"{"motion":1,"dur":"1s","layers":[],"colour":"#FFF"}"##)
            .contains("unknown key `colour`")
    );
    assert!(
        err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a"}]}"#)
            .contains("needs exactly one of `text` or `rect`")
    );
    assert!(
        err(
            r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}]}"#
        )
        .contains("duplicate id `a`")
    );
    assert!(
        err(r#"{"motion":1,"dur":"1s","layers":[{"id":"Bad Id","rect":"1x1"}]}"#)
            .contains("ids use lowercase letters, digits, `-` and `_`, starting with a letter")
    );
    assert!(
        err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","font":"Inter 10"}]}"#)
            .contains("`font` only applies to text layers")
    );
    assert!(err("not json").starts_with("scene: invalid JSON"));
}

#[test]
fn unknown_key_lists_the_allowed_keys() {
    let e = parse_document(r#"{"motion":1,"dur":"1s","layers":[],"colour":"x"}"#).unwrap_err();
    assert_eq!(e.path.as_deref(), Some("scene"));
    assert!(e.to_string().contains("expected one of motion, size, fps"));
    let e = parse_document(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","x":1}]}"#)
        .unwrap_err();
    assert_eq!(e.path.as_deref(), Some("layers[0]"));
}

#[test]
fn rejects_wrong_types_and_missing_required_keys() {
    let err = |j: &str| parse_document(j).unwrap_err();
    assert!(
        err("[]")
            .to_string()
            .starts_with("scene: must be an object; expected e.g. ")
    );
    assert!(
        err(r#"{"dur":"1s","layers":[]}"#)
            .to_string()
            .contains("missing `motion`")
    );
    assert!(
        err(r#"{"motion":1,"dur":"1s"}"#)
            .to_string()
            .contains("missing `layers`")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":5,"layers":[]}"#).path.as_deref(),
        Some("dur")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","layers":{}}"#)
            .path
            .as_deref(),
        Some("layers")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","layers":[1]}"#)
            .path
            .as_deref(),
        Some("layers[0]")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","layers":[{"rect":"1x1"}]}"#)
            .path
            .as_deref(),
        Some("layers[0]")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","fps":0,"layers":[]}"#)
            .path
            .as_deref(),
        Some("fps")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","fps":"x","layers":[]}"#)
            .path
            .as_deref(),
        Some("fps")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","size":"huge","layers":[]}"#)
            .path
            .as_deref(),
        Some("size")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":3}]}"#)
            .path
            .as_deref(),
        Some("layers[0].text")
    );
    assert_eq!(
        err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","text":"x"}]}"#)
            .path
            .as_deref(),
        Some("layers[0]")
    );
}

#[test]
fn shorthand_failures_keep_their_expected_form() {
    let e = parse_document(r#"{"motion":1,"dur":"soon","layers":[]}"#).unwrap_err();
    assert_eq!(e.path.as_deref(), Some("dur"));
    assert_eq!(
        e.to_string(),
        "dur: `soon` is not a valid time; expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`"
    );
}

/// The full error text of `json`.
fn msg(json: &str) -> String {
    parse_document(json).unwrap_err().to_string()
}

#[test]
fn missing_keys_show_an_example() {
    assert_eq!(
        msg(r#"{"motion":1,"layers":[]}"#),
        r#"scene: missing `dur`; expected e.g. "dur":"5s""#
    );
    assert_eq!(
        msg(r#"{"dur":"1s","layers":[]}"#),
        r#"scene: missing `motion`; expected e.g. "motion":1"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s"}"#),
        r#"scene: missing `layers`; expected e.g. "layers":[]"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"rect":"1x1"}]}"#),
        r#"layers[0]: missing `id`; expected e.g. "id":"title""#
    );
}

#[test]
fn wrong_types_show_the_accepted_form_without_naming_the_key_twice() {
    let layer = |kv: &str| {
        msg(&format!(
            r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","rect":"1x1",{kv}}}]}}"#
        ))
    };
    assert_eq!(
        msg(r#"{"motion":1,"dur":5,"layers":[]}"#),
        "dur: must be a string; expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","size":3,"layers":[]}"#),
        "size: must be a string; expected `720p`, `1080p`, `4k`, `square`, `vertical` or `<width>x<height>`"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","bg":0,"layers":[]}"#),
        "bg: must be a string; expected `#RGB`, `#RRGGBB` or `#RRGGBBAA`"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","fps":"x","layers":[]}"#),
        "fps: must be a number above zero; expected e.g. `30`"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":{}}"#),
        r#"layers: must be an array; expected e.g. "layers":[{"id":"box","rect":"100x100"}]"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[1]}"#),
        r#"layers[0]: must be an object; expected e.g. {"id":"box","rect":"100x100"}"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":3,"rect":"1x1"}]}"#),
        "layers[0].id: must be a string; expected ids use lowercase letters, digits, `-` and `_`, starting with a letter"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":3}]}"#),
        r#"layers[0].text: must be a string; expected e.g. "text":"Hello""#
    );
    assert_eq!(
        layer(r#""fill":1"#),
        "layers[0].fill: must be a string; expected `#RGB`, `#RRGGBB` or `#RRGGBBAA`"
    );
    assert_eq!(
        layer(r#""at":1"#),
        "layers[0].at: must be a string; expected `<x> <y>`, each a number of pixels or `center`, e.g. `center 260`"
    );
    assert!(
        layer(r#""in":true"#).starts_with("layers[0].in: must be a string; expected `<preset>")
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":4}]}"#),
        "layers[0].rect: must be a string; expected `<width>x<height> [r<radius>]`, e.g. `300x400 r24`"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":"x","font":9}]}"#),
        "layers[0].font: must be a string; expected `<family> <size> [<weight>]`, e.g. `Inter 96 700`"
    );
}

#[test]
fn other_structural_errors_show_the_expected_form() {
    assert_eq!(
        msg("[]"),
        r#"scene: must be an object; expected e.g. {"motion":1,"dur":"5s","layers":[]}"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a"}]}"#),
        r#"layers[0]: needs exactly one of `text` or `rect`; expected e.g. "text":"Hello" or "rect":"300x100""#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","font":"Inter 9"}]}"#),
        "layers[0].font: `font` only applies to text layers; expected `font` only next to `text`"
    );
    assert_eq!(
        msg(
            r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}]}"#
        ),
        "layers[1].id: duplicate id `a`; expected every layer to have its own id"
    );
    assert!(msg("nope").starts_with("scene: invalid JSON: "));
    assert!(msg("nope").ends_with(r#"; expected e.g. {"motion":1,"dur":"5s","layers":[]}"#));
}

#[test]
fn size_fps_and_bg_overrides_reach_the_scene() {
    let d = parse_document(
        r##"{"motion":1,"size":"720p","fps":60,"dur":"1s","bg":"#102030","layers":[]}"##,
    )
    .unwrap();
    assert_eq!(
        d.scene.size,
        Size {
            width: 1280,
            height: 720
        }
    );
    assert_eq!(d.scene.fps, 60.0);
    assert_eq!(
        d.scene.background,
        Color {
            r: 0x10,
            g: 0x20,
            b: 0x30,
            a: 255
        }
    );
}

#[test]
fn raw_keeps_the_original_value() {
    let d = parse_document(r#"{"layers":[],"dur":"1s","motion":1}"#).unwrap();
    assert_eq!(d.raw["dur"], "1s");
}

#[test]
fn errors_say_what_kind_of_problem_they_are() {
    let kind = |j: &str| parse_document(j).unwrap_err().kind;
    let layer = |l: &str| format!(r#"{{"motion":1,"dur":"1s","layers":[{l}]}}"#);
    assert_eq!(kind("{"), Kind::Syntax);
    assert_eq!(kind(r#"{"motion":1,"layers":[]}"#), Kind::Missing);
    assert_eq!(
        kind(r#"{"motion":1,"dur":"1s","layers":[],"x":1}"#),
        Kind::UnknownName
    );
    assert_eq!(kind(&layer(r#"{"id":"a"}"#)), Kind::Missing);
    assert_eq!(
        kind(&layer(r#"{"id":"a","text":"t","rect":"1x1"}"#)),
        Kind::Conflict
    );
    assert_eq!(
        kind(&layer(r#"{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}"#)),
        Kind::Conflict
    );
    assert_eq!(
        kind(&layer(r#"{"id":"a","rect":"1x1","in":"zoom"}"#)),
        Kind::UnknownName
    );
    assert_eq!(
        kind(&layer(r#"{"id":"a","rect":"1x1","fill":"red"}"#)),
        Kind::Syntax
    );
}

#[test]
fn wrong_json_types_are_wrong_type() {
    let kind = |j: &str| parse_document(j).unwrap_err().kind;
    let scene = |extra: &str| format!(r#"{{"motion":1,"dur":"1s","layers":[]{extra}}}"#);
    assert_eq!(kind("[]"), Kind::WrongType);
    assert_eq!(kind(r#"{"motion":1,"dur":5,"layers":[]}"#), Kind::WrongType);
    assert_eq!(
        kind(r#"{"motion":1,"dur":"1s","layers":{}}"#),
        Kind::WrongType
    );
    assert_eq!(
        kind(r#"{"motion":1,"dur":"1s","layers":[1]}"#),
        Kind::WrongType
    );
    assert_eq!(kind(&scene(r#","presets":[]"#)), Kind::WrongType);
    // A string of the wrong shape is a syntax error, not a type error.
    assert_eq!(kind(r#"{"motion":1,"dur":"5","layers":[]}"#), Kind::Syntax);
}

#[test]
fn fps_tells_a_wrong_type_from_a_bound() {
    let fps = |v: &str| {
        parse_document(&format!(
            r#"{{"motion":1,"dur":"1s","fps":{v},"layers":[]}}"#
        ))
        .unwrap_err()
    };
    assert_eq!(fps(r#""x""#).kind, Kind::WrongType);
    assert_eq!(fps("true").kind, Kind::WrongType);
    assert_eq!(fps("0").kind, Kind::OutOfRange);
    assert_eq!(fps("-1").kind, Kind::OutOfRange);
    assert_eq!(fps("0").message, fps(r#""x""#).message);
}
