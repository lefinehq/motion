//! Unit tests for `src/parse.rs`: JSON, the document's shape and its keys.

use super::*;
use crate::Kind;
use crate::doc::{AnimDoc, LayerDoc, LayersDoc, PresetDoc, PresetsDoc};
use crate::preset::Start;
use crate::shorthand::{Behaviour, Font, Position, RectShape};
use crate::units::{Color, Size, Time};
use crate::value::ValueType;
use serde_json::json;

/// The error of `json`.
fn err(json: &str) -> Diagnostic {
    parse_document(json).unwrap_err()
}

/// A scene that also has `kv` at the top level.
fn scene(kv: &str) -> String {
    format!(r#"{{"motion":1,"dur":"1s","layers":[],{kv}}}"#)
}

/// A scene with one rect layer that also has `kv`.
fn layer(kv: &str) -> String {
    format!(r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","rect":"1x1",{kv}}}]}}"#)
}

/// Asserts that `json` fails with `kind` at `path`, expecting `form`.
#[track_caller]
fn assert_fails(json: &str, kind: Kind, path: Option<&str>, form: &str) {
    let e = err(json);
    assert_eq!(
        (e.kind, e.path.as_deref(), e.expected.as_deref()),
        (kind, path, Some(form)),
        "{json}"
    );
}

#[test]
fn raw_keeps_the_original_value() {
    let d = parse_document(r#"{"layers":[],"dur":"1s","motion":1}"#).unwrap();
    assert_eq!(d.raw()["dur"], "1s");
}

#[test]
fn from_value_keeps_the_value_and_rejects_an_invalid_one() {
    let raw = json!({"motion": 1, "dur": "1s", "layers": [{"id": "a", "rect": "1x1"}]});
    let d = Document::from_value(raw.clone()).unwrap();
    assert_eq!(d.scene().layers[0].id, "a");
    assert_eq!(d.into_raw(), raw);
    let e = Document::from_value(json!({"motion": 1, "layers": []})).unwrap_err();
    assert_eq!(e.kind, Kind::Missing);
}

#[test]
fn layer_errors_sit_under_the_layer_path() {
    let second = |kv: &str| {
        let json = format!(
            r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","rect":"1x1"}},{{"id":"b","rect":"1x1",{kv}}}]}}"#
        );
        err(&json).path.unwrap()
    };
    let prefix = format!("{}.", key::layer_path(1));
    // One from serde's structure, one from a validation rule, one from a
    // rule that places its own path.
    for kv in [r#""text":5"#, r#""fill":"red""#, r#""id":"a""#] {
        let path = second(kv);
        assert!(path.starts_with(&prefix), "{kv}: {path}");
    }
}

#[test]
fn text_that_is_not_json_is_a_syntax_error() {
    assert_fails("nope", Kind::Syntax, None, SceneDoc::FORM);
    // JSON that is not an object gets the same form.
    assert_fails("[]", Kind::WrongType, None, SceneDoc::FORM);
}

#[test]
fn missing_keys_show_the_key_written() {
    for (json, path, form) in [
        (r#"{"motion":1,"layers":[]}"#, None, r#"e.g. "dur":"5s""#),
        (r#"{"dur":"1s","layers":[]}"#, None, r#"e.g. "motion":1"#),
        (r#"{"motion":1,"dur":"1s"}"#, None, r#"e.g. "layers":[]"#),
        (
            r#"{"motion":1,"dur":"1s","layers":[{"rect":"1x1"}]}"#,
            Some("layers[0]"),
            r#"e.g. "id":"title""#,
        ),
    ] {
        assert_fails(json, Kind::Missing, path, form);
    }
}

#[test]
fn null_is_never_a_left_out_key() {
    for (json, path, form) in [
        (scene(r#""size":null"#), "size", Size::FORM),
        (scene(r#""bg":null"#), "bg", Color::FORM),
        (scene(r#""fps":null"#), "fps", "a number"),
        (scene(r#""presets":null"#), "presets", PresetsDoc::FORM),
        (
            r#"{"motion":1,"dur":null,"layers":[]}"#.to_owned(),
            "dur",
            Time::FORM,
        ),
        (layer(r#""in":null"#), "layers[0].in", Behaviour::FORM),
        (layer(r#""anim":null"#), "layers[0].anim", AnimDoc::FORM),
        (
            r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":null}]}"#.to_owned(),
            "layers[0].text",
            "a string",
        ),
    ] {
        assert_fails(&json, Kind::WrongType, Some(path), form);
    }
}

#[test]
fn unknown_keys_list_the_allowed_keys() {
    assert_fails(
        &scene(r#""colour":"x""#),
        Kind::UnknownName,
        Some("colour"),
        &one_of(&SCENE_KEYS),
    );
    let layer_keys: Vec<&str> = crate::layer_keys().collect();
    assert_fails(
        &layer(r#""x":1"#),
        Kind::UnknownName,
        Some("layers[0].x"),
        &one_of(&layer_keys),
    );
}

#[test]
fn wrong_types_name_the_accepted_form() {
    for (json, path, form) in [
        (
            r#"{"motion":1,"dur":5,"layers":[]}"#.to_owned(),
            "dur",
            Time::FORM,
        ),
        (scene(r#""size":3"#), "size", Size::FORM),
        (scene(r#""bg":0"#), "bg", Color::FORM),
        (scene(r#""fps":"x""#), "fps", "a number"),
        (scene(r#""fps":true"#), "fps", "a number"),
        (
            r#"{"motion":"1","dur":"1s","layers":[]}"#.to_owned(),
            "motion",
            "a number",
        ),
        (
            r#"{"motion":1,"dur":"1s","layers":{}}"#.to_owned(),
            "layers",
            LayersDoc::FORM,
        ),
        (
            r#"{"motion":1,"dur":"1s","layers":[1]}"#.to_owned(),
            "layers[0]",
            LayerDoc::FORM,
        ),
        (
            r#"{"motion":1,"dur":"1s","layers":[{"id":3,"rect":"1x1"}]}"#.to_owned(),
            "layers[0].id",
            "a string",
        ),
        (
            r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":3}]}"#.to_owned(),
            "layers[0].text",
            "a string",
        ),
        (
            r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":1}]}"#.to_owned(),
            "layers[0].rect",
            RectShape::FORM,
        ),
        (
            r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":"x","font":1}]}"#.to_owned(),
            "layers[0].font",
            Font::FORM,
        ),
        (
            layer(r#""fill":1"#),
            "layers[0].fill",
            ValueType::Color.form(),
        ),
        (layer(r#""at":1"#), "layers[0].at", Position::FORM),
        (layer(r#""in":true"#), "layers[0].in", Behaviour::FORM),
        (scene(r#""presets":[]"#), "presets", PresetsDoc::FORM),
        (scene(r#""presets":{"a":3}"#), "presets.a", PresetDoc::FORM),
        (layer(r#""anim":[]"#), "layers[0].anim", AnimDoc::FORM),
    ] {
        assert_fails(&json, Kind::WrongType, Some(path), form);
    }
    // A string of the wrong shape is a syntax error, not a type error.
    assert_eq!(
        err(r#"{"motion":1,"dur":"5","layers":[]}"#).kind,
        Kind::Syntax
    );
}

#[test]
fn scene_keys_match_the_scene_document() {
    // Every listed key is accepted: whatever else is wrong, it is not unknown.
    for key in SCENE_KEYS {
        let e = scene_from_value(&json!({ key: null })).unwrap_err();
        assert_ne!(e.kind, Kind::UnknownName, "{key}: {e}");
    }
    // An unlisted key is rejected, and the accepted keys are the list.
    let e = scene_from_value(&json!({ "colour": null })).unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(e.expected.as_deref(), Some(one_of(&SCENE_KEYS).as_str()));
}

#[test]
fn layer_keys_match_the_layer_document() {
    let scene = |layer| json!({"motion": 1, "dur": "1s", "layers": [layer]});
    for key in LAYER_KEYS {
        let e = scene_from_value(&scene(json!({ key: null }))).unwrap_err();
        assert_ne!(e.kind, Kind::UnknownName, "{key}: {e}");
    }
    // A property name holds the property's resting value.
    for key in crate::property::property_names() {
        let e = scene_from_value(&scene(json!({ key: null }))).unwrap_err();
        assert_ne!(e.kind, Kind::UnknownName, "{key}: {e}");
    }
    let e = scene_from_value(&scene(json!({ "colour": null }))).unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    let keys: Vec<&str> = crate::layer_keys().collect();
    assert_eq!(e.expected.as_deref(), Some(one_of(&keys).as_str()));
}

/// The accepted form of an unknown key among `keys`, as
/// `Diagnostic::unknown` lists names.
fn one_of(keys: &[&str]) -> String {
    format!("one of {}", keys.join(", "))
}

#[test]
fn a_value_that_reads_like_a_form_stays_a_value() {
    assert_fails(
        &scene(r#""fps":"a, expected b""#),
        Kind::WrongType,
        Some("fps"),
        "a number",
    );
    assert_fails(
        &scene(r#""presets":{"a":{"x":"l, expected q"}}"#),
        Kind::Invalid,
        Some("presets.a.x"),
        Start::form(),
    );
}

#[test]
fn every_serde_shape_maps_to_its_kind() {
    let invalid_value = err(&scene(r#""presets":{"a":{"x":"left"}}"#));
    assert_eq!(invalid_value.kind, Kind::Invalid);
    let short_key = err(&layer(r#""anim":{"x":[[0]]}"#));
    assert_eq!(short_key.kind, Kind::Invalid);
    assert_eq!(short_key.path.as_deref(), Some("layers[0].anim.x[0]"));
}

mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_document_any_text(s in ".{0,200}") {
            let _ = parse_document(&s);
        }

        #[test]
        fn parse_document_scene_shaped_text(
            dur in "[0-9.a-z]{0,4}",
            fps in "[-0-9.\"a-z]{0,4}",
            id in "[ -~]{0,6}",
            kv in "[ -~]{0,30}",
        ) {
            let json = format!(
                r#"{{"motion":1,"dur":"{dur}","fps":{fps},"layers":[{{"id":"{id}",{kv}}}]}}"#
            );
            let _ = parse_document(&json);
        }
    }
}

#[test]
fn documents_are_objects_not_arrays_of_field_values() {
    // Serde's derived structs would read their fields, in order, from an array.
    let e = scene_from_value(&json!([1, null, null, "1s", null, {}, []])).unwrap_err();
    assert_eq!(e.kind, Kind::WrongType);
    let e =
        scene_from_value(&json!({"motion": 1, "dur": "1s", "layers": [["a", "hi"]]})).unwrap_err();
    assert_eq!(
        (e.kind, e.path.as_deref(), e.expected.as_deref()),
        (Kind::WrongType, Some("layers[0]"), Some(LayerDoc::FORM))
    );
}
