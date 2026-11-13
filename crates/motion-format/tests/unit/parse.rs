//! Unit tests for `src/parse.rs`: JSON, serde's structural errors and the
//! one mapping from them to a diagnostic.

use super::*;
use serde_json::json;

/// The error of `json`.
fn err(json: &str) -> Diagnostic {
    parse_document(json).unwrap_err()
}

/// The full error text of `json`.
fn msg(json: &str) -> String {
    err(json).to_string()
}

/// A scene with one rect layer that also has `kv`.
fn layer(kv: &str) -> String {
    msg(&format!(
        r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","rect":"1x1",{kv}}}]}}"#
    ))
}

#[test]
fn raw_keeps_the_original_value() {
    let d = parse_document(r#"{"layers":[],"dur":"1s","motion":1}"#).unwrap();
    assert_eq!(d.raw["dur"], "1s");
}

#[test]
fn text_that_is_not_json_is_a_syntax_error() {
    let e = err("nope");
    assert_eq!(e.kind, Kind::Syntax);
    assert_eq!(e.path, None);
    let text = e.to_string();
    assert!(text.starts_with("invalid JSON: "), "{text}");
    // Text that is not JSON and JSON that is not an object get the same form.
    assert_eq!(e.expected, err("[]").expected);
}

#[test]
fn missing_keys_name_the_key() {
    assert_eq!(
        msg(r#"{"motion":1,"layers":[]}"#),
        "missing field `dur`; expected the key `dur`"
    );
    assert_eq!(
        msg(r#"{"dur":"1s","layers":[]}"#),
        "missing field `motion`; expected the key `motion`"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s"}"#),
        "missing field `layers`; expected the key `layers`"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"rect":"1x1"}]}"#),
        "layers[0]: missing field `id`; expected the key `id`"
    );
    assert_eq!(err(r#"{"motion":1,"layers":[]}"#).kind, Kind::Missing);
}

#[test]
fn unknown_keys_list_the_allowed_keys() {
    let e = err(r#"{"motion":1,"dur":"1s","layers":[],"colour":"x"}"#);
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(
        e.to_string(),
        "colour: unknown field `colour`; expected one of `motion`, `size`, `fps`, `dur`, `bg`, `presets`, `layers`"
    );
    assert_eq!(
        layer(r#""x":1"#),
        "layers[0].x: unknown field `x`; expected one of `id`, `text`, `rect`, `font`, `fill`, `at`, `in`, `anim`"
    );
}

#[test]
fn wrong_types_name_what_was_found_and_the_accepted_form() {
    assert_eq!(
        msg("[]"),
        r#"invalid type: sequence; expected a scene object, e.g. {"motion":1,"dur":"5s","layers":[]}"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":5,"layers":[]}"#),
        "dur: invalid type: integer `5`; expected a string"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","size":3,"layers":[]}"#),
        "size: invalid type: integer `3`; expected a string"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","bg":0,"layers":[]}"#),
        "bg: invalid type: integer `0`; expected a string"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","fps":"x","layers":[]}"#),
        r#"fps: invalid type: string "x"; expected a number"#
    );
    assert_eq!(
        msg(r#"{"motion":"1","dur":"1s","layers":[]}"#),
        r#"motion: invalid type: string "1"; expected a number"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":{}}"#),
        "layers: invalid type: map; expected a sequence"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[1]}"#),
        r#"layers[0]: invalid type: integer `1`; expected a layer object, e.g. {"id":"box","rect":"100x100"}"#
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":3,"rect":"1x1"}]}"#),
        "layers[0].id: invalid type: integer `3`; expected a string"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":3}]}"#),
        "layers[0].text: invalid type: integer `3`; expected a string"
    );
    assert_eq!(
        layer(r#""fill":1"#),
        "layers[0].fill: invalid type: integer `1`; expected a string"
    );
    assert_eq!(
        layer(r#""in":true"#),
        "layers[0].in: invalid type: boolean `true`; expected a string"
    );
    assert_eq!(
        msg(r#"{"motion":1,"dur":"1s","presets":[],"layers":[]}"#),
        "presets: invalid type: sequence; expected a map"
    );
}

#[test]
fn wrong_json_types_are_wrong_type() {
    let kind = |j: &str| err(j).kind;
    let scene = |extra: &str| format!(r#"{{"motion":1,"dur":"1s","layers":[]{extra}}}"#);
    assert_eq!(kind("[]"), Kind::WrongType);
    assert_eq!(kind(r#"{"motion":1,"dur":5,"layers":[]}"#), Kind::WrongType);
    assert_eq!(
        kind(r#"{"motion":"1","dur":"1s","layers":[]}"#),
        Kind::WrongType
    );
    assert_eq!(kind(&scene(r#","fps":true"#)), Kind::WrongType);
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
    let e = scene_from_value(&scene(json!({ "colour": null }))).unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(e.expected.as_deref(), Some(one_of(&LAYER_KEYS).as_str()));
}

/// The accepted form serde gives for an unknown field among `keys`.
fn one_of(keys: &[&str]) -> String {
    let keys: Vec<String> = keys.iter().map(|k| format!("`{k}`")).collect();
    format!("one of {}", keys.join(", "))
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
        e.to_string(),
        r#"layers[0]: invalid type: sequence; expected a layer object, e.g. {"id":"box","rect":"100x100"}"#
    );
}
