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
    assert!(err("card colour=#FFF").contains(
        "unknown key `colour` for a layer; allowed: id, text, rect, font, fill, at, in, anim"
    ));
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

fn with_anim() -> Value {
    run(r#"card in=none anim.opacity=[["0f",0],["12f",1,"ease-out"]]"#)
}

#[test]
fn json_array_round_trips_into_a_valid_scene() {
    let out = with_anim();
    assert_eq!(
        out["layers"][1]["anim"]["opacity"],
        json!([["0f", 0], ["12f", 1, "ease-out"]])
    );
    assert!(motion_format::scene_from_value(&out).is_ok());
}

#[test]
fn preset_defined_then_used() {
    let out = run(r#"scene presets.my-pop={"opacity":0,"scale":0.5}"#);
    assert_eq!(out["presets"]["my-pop"], json!({"opacity":0,"scale":0.5}));
    let out = apply_edit(&out, &parse_edit("card in=my-pop").unwrap()).unwrap();
    assert_eq!(out["layers"][1]["in"], "my-pop");
}

#[test]
fn removing_nested_keys_cleans_up_empty_objects() {
    let base = with_anim();
    let out = apply_edit(&base, &parse_edit("card anim.opacity=none").unwrap()).unwrap();
    assert!(out["layers"][1].get("anim").is_none());
    // Siblings keep the parent alive.
    let two = apply_edit(
        &base,
        &parse_edit("card anim.scale=[[\"0f\",1],[\"6f\",2]]").unwrap(),
    )
    .unwrap();
    let out = apply_edit(&two, &parse_edit("card anim.opacity=none").unwrap()).unwrap();
    assert_eq!(keys(&out["layers"][1]["anim"]), ["scale"]);
    // Cleanup climbs several levels, but never removes the scene itself.
    let raw = json!({"a": {"b": {"c": 1}}});
    let mut m = raw.as_object().unwrap().clone();
    assert!(remove_path(&mut m, &["a", "b", "c"]));
    assert!(m.is_empty());
}

#[test]
fn removing_a_missing_path_is_a_no_op() {
    assert_eq!(run("card anim.opacity=none"), doc());
    assert_eq!(run("card in.x=none"), doc());
    let raw = json!({"a": {}});
    let mut m = raw.as_object().unwrap().clone();
    assert!(!remove_path(&mut m, &["a", "x"]));
    assert_eq!(m.len(), 1);
}

#[test]
fn setting_through_a_non_object_is_an_error() {
    assert_eq!(
        err("card in.opacity=1"),
        "cannot set `in.opacity`: `in` is not an object"
    );
}

#[test]
fn only_the_first_segment_is_checked_as_a_key() {
    assert!(err("card nope.x=1").contains("unknown key `nope` for a layer; allowed:"));
    let m = err("card anim.nope=1");
    assert!(m.starts_with("invalid result: "), "{m}");
}

#[test]
fn errors_say_what_kind_of_problem_they_are() {
    let kind = |l: &str| {
        apply_edit(&doc(), &parse_edit(l).unwrap())
            .unwrap_err()
            .kind
    };
    assert_eq!(kind("ghost fill=red"), Kind::UnknownName);
    assert_eq!(kind("card colour=red"), Kind::UnknownName);
    // A validation failure keeps the kind it had in the format.
    assert_eq!(kind("card fill=blue"), Kind::Syntax);
    assert_eq!(kind("title text=2024"), Kind::Invalid);
}
