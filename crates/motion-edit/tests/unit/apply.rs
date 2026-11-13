//! Unit tests for `src/apply.rs`.

use super::*;
use crate::parse_edit;
use serde_json::json;

fn doc() -> Value {
    motion_format::parse_document(include_str!("../../../../examples/launch.json"))
        .unwrap()
        .raw
}
fn run(l: &str) -> Value {
    apply_edit(&doc(), &parse_edit(l).unwrap()).unwrap()
}
fn err(l: &str) -> String {
    apply_edit(&doc(), &parse_edit(l).unwrap())
        .unwrap_err()
        .to_string()
}
fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

#[test]
fn sets_and_removes_layer_keys() {
    let out = apply_edit(&doc(), &parse_edit(r#"card fill=#FF0000 in=none"#).unwrap()).unwrap();
    assert_eq!(out["layers"][1]["fill"], "#FF0000");
    assert!(out["layers"][1].get("in").is_none());
    assert_eq!(out["layers"][0], doc()["layers"][0]); // untouched layers stay byte-identical
}

#[test]
fn edits_the_scene_itself() {
    assert_eq!(
        apply_edit(&doc(), &parse_edit("scene bg=#000000 fps=60").unwrap()).unwrap()["fps"],
        60
    );
}

#[test]
fn reports_unknown_targets_keys_and_invalid_results() {
    let err = |l: &str| {
        apply_edit(&doc(), &parse_edit(l).unwrap())
            .unwrap_err()
            .to_string()
    };
    assert_eq!(err("ghost fill=#FFF"), "no layer with id `ghost`");
    assert!(
        err("card colour=#FFF").contains(
            "unknown key `colour` for a layer; allowed: id, text, rect, font, fill, at, in"
        )
    );
    assert!(err("card fill=blue").starts_with("invalid result: layers[1].fill"));
    assert!(err("card id=title").contains("duplicate id `title`"));
}

#[test]
fn existing_key_keeps_its_position() {
    let out = run("card rect=\"10x10\"");
    assert_eq!(keys(&out["layers"][1]), keys(&doc()["layers"][1]));
    assert_eq!(out["layers"][1]["rect"], "10x10");
}

#[test]
fn new_key_is_appended() {
    let out = run("title fill=#FFFFFF");
    let k = keys(&out["layers"][0]);
    assert_eq!(k.last(), Some(&"fill"));
    assert_eq!(k.len(), keys(&doc()["layers"][0]).len() + 1);
}

#[test]
fn removing_a_missing_key_is_a_no_op() {
    assert_eq!(run("title fill=none"), doc());
}

#[test]
fn scene_keys_are_validated_against_scene_keys() {
    assert!(err("scene colour=red").contains(
        "unknown key `colour` for the scene; allowed: motion, size, fps, dur, bg, presets, layers"
    ));
    // A layer key is not a scene key.
    assert!(err("scene fill=#FFF").contains("unknown key `fill` for the scene"));
}

#[test]
fn unknown_keys_are_reported_before_unknown_targets() {
    assert!(err("ghost colour=red").contains("unknown key `colour`"));
}

#[test]
fn invalid_scene_result_is_reported() {
    assert!(err("scene fps=\"fast\"").starts_with("invalid result: "));
}

#[test]
fn number_or_bool_for_a_string_key_gets_a_quote_hint() {
    assert_eq!(
        err("title text=2024"),
        "invalid result: layers[0].text: must be a string; expected e.g. \"text\":\"Hello\"; quote it: text=\"2024\""
    );
    assert!(err("title text=true").ends_with("; quote it: text=\"true\""));
    assert!(err("scene bg=1.5").ends_with("; quote it: bg=\"1.5\""));
    // Other errors, and strings that are simply wrong, get no hint.
    assert!(!err("card fill=blue").contains("quote it"));
    assert!(!err("scene fps=\"fast\"").contains("quote it"));
}

#[test]
fn input_is_not_mutated() {
    let before = doc();
    let _ = apply_edit(&before, &parse_edit("card fill=#000000 in=none").unwrap());
    assert_eq!(before, doc());
}

#[test]
fn numbers_and_bools_are_written_as_json() {
    assert_eq!(run("scene fps=24")["fps"], json!(24));
}

#[test]
fn missing_layers_array_is_an_error_not_a_panic() {
    let raw = json!({"motion": 1});
    let e = apply_edit(&raw, &parse_edit("card fill=#FFF").unwrap()).unwrap_err();
    assert!(e.to_string().contains("no `layers` array"));
}

#[test]
fn non_object_document_is_an_error() {
    let e = apply_edit(&json!([1]), &parse_edit("scene fps=1").unwrap()).unwrap_err();
    assert!(e.to_string().contains("not an object"));
}
