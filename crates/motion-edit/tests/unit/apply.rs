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
    assert_eq!(
        err("ghost fill=#FFF"),
        "unknown layer id `ghost`; expected one of title, card"
    );
    assert_eq!(
        err("card colour=#FFF"),
        "unknown layer key `colour`; expected one of id, text, rect, font, fill, at, in, anim"
    );
    assert!(err("card fill=blue").starts_with("layers[1].fill: invalid result: "));
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
    assert_eq!(
        err("scene colour=red"),
        "unknown scene key `colour`; expected one of motion, size, fps, dur, bg, presets, layers"
    );
    // A layer key is not a scene key.
    assert!(err("scene fill=#FFF").starts_with("unknown scene key `fill`"));
}

#[test]
fn unknown_keys_are_reported_before_unknown_targets() {
    assert!(err("ghost colour=red").starts_with("unknown layer key `colour`"));
}

#[test]
fn invalid_scene_result_is_reported() {
    assert_eq!(
        err("scene fps=\"fast\""),
        r#"fps: invalid result: invalid type: string "fast"; expected a number"#
    );
}

#[test]
fn invalid_result_keeps_the_validation_diagnostic() {
    let e = apply_edit(&doc(), &parse_edit("card fill=blue").unwrap()).unwrap_err();
    assert_eq!(e.kind, Kind::Syntax);
    assert_eq!(e.path.as_deref(), Some("layers[1].fill"));
    assert!(e.message.starts_with("invalid result: "), "{}", e.message);
    assert_eq!(
        e.expected.as_deref(),
        Some("`#RGB`, `#RRGGBB` or `#RRGGBBAA`")
    );
}

#[test]
fn number_or_bool_for_a_string_key_gets_a_quote_hint() {
    assert_eq!(
        err("title text=2024"),
        "layers[0].text: invalid result: invalid type: integer `2024`; expected a quoted value: `text=\"2024\"`"
    );
    let e = apply_edit(&doc(), &parse_edit("title text=2024").unwrap()).unwrap_err();
    assert_eq!(e.kind, Kind::WrongType);
    assert!(err("title text=true").ends_with("; expected a quoted value: `text=\"true\"`"));
    // Other errors, and strings that are simply wrong, get no hint.
    assert!(!err("card fill=blue").contains("quoted"));
    assert!(!err("scene fps=\"fast\"").contains("quoted"));
}

#[test]
fn a_quote_that_would_still_fail_there_gets_the_form_instead() {
    // Quoted, `5` is a time without a unit and `1.5` is not a colour: the
    // key's own form helps more than a quote.
    assert_eq!(
        err("scene dur=5"),
        "dur: invalid result: invalid type: integer `5`; expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`"
    );
    assert_eq!(
        err("scene bg=1.5"),
        "bg: invalid result: invalid type: floating point `1.5`; expected `#RGB`, `#RRGGBB` or `#RRGGBBAA`"
    );
}

#[test]
fn a_number_for_an_unknown_property_gets_the_track_form() {
    // `anim.nope=1` is a wrong type first; quoting it would leave an unknown
    // property at the same path, so there is no quote hint.
    let e = apply_edit(&doc(), &parse_edit("card anim.nope=1").unwrap()).unwrap_err();
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(
        e.to_string(),
        r#"layers[1].anim.nope: invalid result: invalid type: integer `1`; expected "<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out", or a list of [time, value] keys"#
    );
}

#[test]
fn a_wrong_type_on_a_number_key_gets_no_quote_hint() {
    // Quoting `true` would leave `fps` just as wrong, so no hint is given.
    let e = apply_edit(&doc(), &parse_edit("scene fps=true").unwrap()).unwrap_err();
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(
        e.to_string(),
        "fps: invalid result: invalid type: boolean `true`; expected a number"
    );
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
    assert!(err("card nope.x=1").starts_with("unknown layer key `nope`; expected one of "));
    let m = err("card anim.nope=[[0,1]]");
    assert!(
        m.starts_with("layers[1].anim.nope: invalid result: unknown property `nope`"),
        "{m}"
    );
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
    assert_eq!(kind("title text=2024"), Kind::WrongType);
    assert_eq!(kind("card in.opacity=1"), Kind::WrongType);
}
