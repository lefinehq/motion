//! Unit tests for `src/apply.rs`. The wording of the errors is pinned in the
//! `every_error_message` snapshot of `parse.rs`.

use super::*;
use crate::parse_edit;
use serde_json::json;

fn doc() -> Document {
    motion_format::parse_document(include_str!("../../../../examples/launch.json")).unwrap()
}
fn raw() -> Value {
    doc().into_raw()
}
fn edited(l: &str) -> Document {
    apply_edit(&doc(), &parse_edit(l).unwrap()).unwrap()
}
fn run(l: &str) -> Value {
    edited(l).into_raw()
}
fn fail(l: &str) -> Diagnostic {
    apply_edit(&doc(), &parse_edit(l).unwrap()).unwrap_err()
}
/// What validation alone says about the edited scene, before any hint.
fn invalid(l: &str) -> Diagnostic {
    let (raw, _) = apply_changes(&doc(), &parse_edit(l).unwrap()).unwrap();
    Document::from_value(raw).unwrap_err()
}
fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

#[test]
fn sets_and_removes_layer_keys() {
    let out = run("card fill=#FF0000 in=none");
    assert_eq!(out["layers"][1]["fill"], "#FF0000");
    assert!(out["layers"][1].get("in").is_none());
    assert_eq!(out["layers"][0], raw()["layers"][0]); // untouched layers stay byte-identical
}

#[test]
fn edits_the_scene_itself() {
    assert_eq!(run("scene bg=#000000 fps=60")["fps"], 60);
}

#[test]
fn reports_unknown_targets_keys_and_invalid_results() {
    assert_eq!(
        fail("ghost fill=#FFF"),
        Diagnostic::unknown("layer id", "ghost", ["title", "card"])
    );
    assert_eq!(
        fail("card colour=#FFF"),
        Diagnostic::unknown("layer key", "colour", LAYER_KEYS)
    );
    let colour = fail("card fill=blue");
    assert_eq!(colour.path.as_deref(), Some("layers[1].fill"));
    assert_eq!(colour, invalid("card fill=blue"));
    let duplicate = fail("card id=title");
    assert_eq!(duplicate.kind, Kind::Conflict);
    assert_eq!(duplicate, invalid("card id=title"));
}

#[test]
fn existing_key_keeps_its_position() {
    let out = run("card rect=\"10x10\"");
    assert_eq!(keys(&out["layers"][1]), keys(&raw()["layers"][1]));
    assert_eq!(out["layers"][1]["rect"], "10x10");
}

#[test]
fn new_key_is_appended() {
    let out = run("title fill=#FFFFFF");
    let k = keys(&out["layers"][0]);
    assert_eq!(k.last(), Some(&"fill"));
    assert_eq!(k.len(), keys(&raw()["layers"][0]).len() + 1);
}

#[test]
fn removing_a_missing_key_is_a_no_op() {
    assert_eq!(run("title fill=none"), raw());
}

#[test]
fn scene_keys_are_validated_against_scene_keys() {
    assert_eq!(
        fail("scene colour=red"),
        Diagnostic::unknown("scene key", "colour", SCENE_KEYS)
    );
    // A layer key is not a scene key.
    assert_eq!(
        fail("scene fill=#FFF"),
        Diagnostic::unknown("scene key", "fill", SCENE_KEYS)
    );
}

#[test]
fn unknown_keys_are_reported_before_unknown_targets() {
    assert_eq!(
        fail("ghost colour=red"),
        Diagnostic::unknown("layer key", "colour", LAYER_KEYS)
    );
}

#[test]
fn invalid_scene_result_is_reported() {
    let e = fail("scene fps=\"fast\"");
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(e.path.as_deref(), Some("fps"));
    assert_eq!(e, invalid("scene fps=\"fast\""));
}

#[test]
fn invalid_result_is_the_validation_diagnostic() {
    let e = fail("card fill=blue");
    assert_eq!(e.kind, Kind::Syntax);
    assert_eq!(e.path.as_deref(), Some("layers[1].fill"));
    assert_eq!(e, invalid("card fill=blue"));
}

#[test]
fn number_or_bool_for_a_string_key_gets_a_quote_hint() {
    for (line, quoted) in [
        ("title text=2024", r#"`text="2024"`"#),
        ("title text=true", r#"`text="true"`"#),
    ] {
        let e = fail(line);
        let plain = invalid(line);
        assert_eq!(e.kind, Kind::WrongType, "{line}");
        assert_eq!(e.path.as_deref(), Some("layers[0].text"), "{line}");
        assert_eq!(e.message, plain.message, "{line}");
        let expected = e.expected.as_deref().unwrap_or_default();
        assert!(expected.ends_with(quoted), "{line}: {expected}");
    }
    // Other errors, and strings that are simply wrong, get no hint.
    assert_eq!(fail("card fill=blue"), invalid("card fill=blue"));
    assert_eq!(fail("scene fps=\"fast\""), invalid("scene fps=\"fast\""));
}

#[test]
fn a_quote_that_would_still_fail_there_gets_the_form_instead() {
    // Quoted, `5` is a time without a unit and `1.5` is not a colour: the
    // key's own form helps more than a quote.
    for line in ["scene dur=5", "scene bg=1.5"] {
        let e = fail(line);
        assert_eq!(e.kind, Kind::WrongType, "{line}");
        assert_eq!(e, invalid(line), "{line}");
    }
}

#[test]
fn a_number_for_an_unknown_property_gets_the_track_form() {
    // Quoting it would leave an unknown property at the same path.
    let e = fail("card anim.nope=1");
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(e.path.as_deref(), Some("layers[1].anim.nope"));
    assert_eq!(e, invalid("card anim.nope=1"));
}

#[test]
fn a_wrong_type_on_a_number_key_gets_no_quote_hint() {
    // Quoting `true` would leave `fps` just as wrong.
    let e = fail("scene fps=true");
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(e, invalid("scene fps=true"));
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

fn with_anim() -> Document {
    edited(r#"card in=none anim.opacity=[["0f",0],["12f",1,"ease-out"]]"#)
}

#[test]
fn json_array_round_trips_into_a_valid_scene() {
    let out = with_anim().into_raw();
    assert_eq!(
        out["layers"][1]["anim"]["opacity"],
        json!([["0f", 0], ["12f", 1, "ease-out"]])
    );
    assert!(Document::from_value(out).is_ok());
}

#[test]
fn preset_defined_then_used() {
    let out = edited(r#"scene presets.my-pop={"opacity":0,"scale":0.5}"#);
    assert_eq!(
        out.raw()["presets"]["my-pop"],
        json!({"opacity":0,"scale":0.5})
    );
    let out = apply_edit(&out, &parse_edit("card in=my-pop").unwrap())
        .unwrap()
        .into_raw();
    assert_eq!(out["layers"][1]["in"], "my-pop");
}

#[test]
fn removing_nested_keys_cleans_up_empty_objects() {
    let base = with_anim();
    let out = apply_edit(&base, &parse_edit("card anim.opacity=none").unwrap())
        .unwrap()
        .into_raw();
    assert!(out["layers"][1].get("anim").is_none());
    // Siblings keep the parent alive.
    let two = apply_edit(
        &base,
        &parse_edit("card anim.scale=[[\"0f\",1],[\"6f\",2]]").unwrap(),
    )
    .unwrap();
    let out = apply_edit(&two, &parse_edit("card anim.opacity=none").unwrap())
        .unwrap()
        .into_raw();
    assert_eq!(keys(&out["layers"][1]["anim"]), ["scale"]);
}

#[test]
fn removing_a_missing_path_is_a_no_op() {
    assert_eq!(run("card anim.opacity=none"), raw());
    assert_eq!(run("card in.x=none"), raw());
}

#[test]
fn setting_through_a_non_object_is_an_error() {
    let e = fail("card in.opacity=1");
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(e.path, None);
    assert!(e.message.contains("`in`"), "{e}");
}

#[test]
fn only_the_first_segment_is_checked_as_a_key() {
    assert_eq!(
        fail("card nope.x=1"),
        Diagnostic::unknown("layer key", "nope", LAYER_KEYS)
    );
    let e = fail("card anim.nope=[[0,1]]");
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(e.path.as_deref(), Some("layers[1].anim.nope"));
}

#[test]
fn errors_say_what_kind_of_problem_they_are() {
    let kind = |l: &str| fail(l).kind;
    assert_eq!(kind("ghost fill=red"), Kind::UnknownName);
    assert_eq!(kind("card colour=red"), Kind::UnknownName);
    // A validation failure keeps the kind it had in the format.
    assert_eq!(kind("card fill=blue"), Kind::Syntax);
    assert_eq!(kind("title text=2024"), Kind::WrongType);
    assert_eq!(kind("card in.opacity=1"), Kind::WrongType);
}
