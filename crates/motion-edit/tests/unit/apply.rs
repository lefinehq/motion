//! Unit tests for `src/apply.rs`. The wording of the errors is pinned in the
//! `every_error_message` snapshot of `parse.rs`.

use super::*;
use crate::parse_edit;
use motion_format::layer_keys;
use pretty_assertions::assert_eq;
use rstest::rstest;
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
        Diagnostic::unknown("layer", "ghost", ["title", "card"])
    );
    assert_eq!(
        fail("card colour=#FFF"),
        Diagnostic::unknown("layer key", "colour", layer_keys())
    );
    let colour = fail("card fill=blue");
    assert_eq!(colour.path_text().as_deref(), Some("card.fill"));
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

#[rstest]
#[case::missing_key("title fill=none")]
#[case::missing_nested_key("card anim.opacity=none")]
#[case::path_through_a_string("card in.x=none")]
fn removing_a_missing_key_or_path_is_a_no_op(#[case] line: &str) {
    assert_eq!(run(line), raw());
}

#[rstest]
#[case::scene_keys_are_validated_against_scene_keys(
    "scene colour=red",
    "scene key",
    "colour",
    &SCENE_KEYS
)]
// A layer key is not a scene key.
#[case::a_layer_key_on_the_scene("scene fill=#FFF", "scene key", "fill", &SCENE_KEYS)]
#[case::before_an_unknown_target("ghost colour=red", "layer key", "colour", &layer_keys().collect::<Vec<_>>())]
fn unknown_keys_are_named_with_the_targets_keys(
    #[case] line: &str,
    #[case] what: &str,
    #[case] key: &str,
    #[case] allowed: &[&str],
) {
    assert_eq!(fail(line), Diagnostic::unknown(what, key, allowed));
}

#[rstest]
#[case::an_invalid_scene_result("scene fps=\"fast\"", Kind::WrongType, "fps")]
#[case::an_invalid_layer_result("card fill=blue", Kind::Syntax, "card.fill")]
// Quoting it would leave an unknown property at the same path.
#[case::a_number_for_an_unknown_property_gets_the_track_form(
    "card anim.nope=1",
    Kind::WrongType,
    "card.anim.nope"
)]
fn invalid_result_is_the_validation_diagnostic(
    #[case] line: &str,
    #[case] kind: Kind,
    #[case] path: &str,
) {
    let e = fail(line);
    assert_eq!(e.kind, kind);
    assert_eq!(e.path_text().as_deref(), Some(path));
    assert_eq!(e, invalid(line));
}

#[rstest]
#[case::number("title text=2024", r#"`text="2024"`"#)]
#[case::boolean("title text=true", r#"`text="true"`"#)]
fn number_or_bool_for_a_string_key_gets_a_quote_hint(#[case] line: &str, #[case] quoted: &str) {
    let e = fail(line);
    let plain = invalid(line);
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(e.path_text().as_deref(), Some("title.text"));
    assert_eq!(e.message, plain.message);
    let expected = e.expected.as_deref().unwrap_or_default();
    assert!(expected.ends_with(quoted), "{expected}");
}

#[rstest]
// Other errors, and strings that are simply wrong, get no hint.
#[case::another_error("card fill=blue")]
#[case::a_wrong_string("scene fps=\"fast\"")]
fn other_errors_get_no_quote_hint(#[case] line: &str) {
    assert_eq!(fail(line), invalid(line));
}

#[rstest]
// Quoted, `5` is a time without a unit and `1.5` is not a colour: the
// key's own form helps more than a quote.
#[case::a_time_without_a_unit("scene dur=5")]
#[case::a_number_for_a_colour("scene bg=1.5")]
// Quoting `true` would leave `fps` just as wrong.
#[case::a_wrong_type_on_a_number_key_gets_no_quote_hint("scene fps=true")]
fn a_quote_that_would_still_fail_there_gets_the_form_instead(#[case] line: &str) {
    let e = fail(line);
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(e, invalid(line));
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
        Diagnostic::unknown("layer key", "nope", layer_keys())
    );
    let e = fail("card anim.nope=[[0,1]]");
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(e.path_text().as_deref(), Some("card.anim.nope"));
}

#[rstest]
#[case("ghost fill=red", Kind::UnknownName)]
#[case("card colour=red", Kind::UnknownName)]
// A validation failure keeps the kind it had in the format.
#[case("card fill=blue", Kind::Syntax)]
#[case("title text=2024", Kind::WrongType)]
#[case("card in.opacity=1", Kind::WrongType)]
fn errors_say_what_kind_of_problem_they_are(#[case] line: &str, #[case] kind: Kind) {
    assert_eq!(fail(line).kind, kind);
}

/// A scene where `label` (a title and a badge) is drawn by `first` and
/// `second`, and inside `card` by `deep`.
fn instances() -> Document {
    motion_format::parse_document(
        r##"{"motion":1,"dur":"1s","comps":{
          "label":{"size":"100x50","dur":"1s","layers":[
            {"id":"title","text":"Shared","anim":{"opacity":"0 -> 1 @0-5f","x":"0 -> 10 @0-5f"}},
            {"id":"badge","rect":"10x10","fill":"#00FF00","fx":[{"id":"tint","fill":"#FFFFFF"}]}]},
          "card":{"size":"200x100","dur":"1s","layers":[{"id":"label","comp":"label"}]}},
        "layers":[{"id":"first","comp":"label"},{"id":"second","comp":"label"},{"id":"deep","comp":"card"}]}"##,
    )
    .unwrap()
}

fn on_instances(line: &str) -> Result<Document> {
    apply_edit(&instances(), &parse_edit(line).unwrap())
}

#[test]
fn an_inner_layer_is_edited_for_one_instance_through_overrides() {
    let out = on_instances("second/title text=Sale").unwrap().into_raw();
    assert_eq!(
        out["layers"][1]["overrides"],
        json!({"title": {"text": "Sale"}})
    );
    // The composition and the other instance are as they were.
    assert_eq!(out["comps"], instances().raw()["comps"]);
    assert_eq!(out["layers"][0], instances().raw()["layers"][0]);
    // Deeper, the override goes on the scene's layer the address starts at.
    let out = on_instances("deep/label/title text=Deep")
        .unwrap()
        .into_raw();
    assert_eq!(
        out["layers"][2]["overrides"],
        json!({"label/title": {"text": "Deep"}})
    );
}

#[test]
fn a_change_inside_a_key_starts_from_the_key_as_the_instance_draws_it() {
    let out = on_instances(r#"second/title anim.x="5 -> 6 @0-5f""#)
        .unwrap()
        .into_raw();
    // The whole `anim` is the override's, so the other track comes along.
    assert_eq!(
        out["layers"][1]["overrides"]["title"]["anim"],
        json!({"opacity": "0 -> 1 @0-5f", "x": "5 -> 6 @0-5f"})
    );
    let out = on_instances("second/badge fx[tint].fill=#000000")
        .unwrap()
        .into_raw();
    assert_eq!(
        out["layers"][1]["overrides"]["badge"]["fx"],
        json!([{"id": "tint", "fill": "#000000"}])
    );
}

#[test]
fn none_through_an_instance_takes_back_its_own_value_only() {
    let set = on_instances("second/badge fill=#FF0000").unwrap();
    // Taking back the instance's fill leaves no empty override behind ...
    let back = apply_edit(&set, &parse_edit("second/badge fill=none").unwrap());
    let err = back.unwrap_err();
    // ... but the composition still gives the badge a fill, so it stays.
    assert_eq!(err.kind, Kind::Conflict);
    assert_eq!(
        err.expected.as_deref(),
        Some(
            "`second/badge fill=<value>`, or `scene comps.label.layers[badge].fill=none` for every layer that draws `label`"
        )
    );
    // A key only the override set goes away, and so does the empty override.
    let set = on_instances(r#"second/title hidden=true"#).unwrap();
    let back = apply_edit(&set, &parse_edit("second/title hidden=none").unwrap()).unwrap();
    assert!(back.raw()["layers"][1].get("overrides").is_none());
}

#[test]
fn a_selector_reaches_into_a_list() {
    let out = on_instances("scene comps.label.layers[title].text=Hi")
        .unwrap()
        .into_raw();
    assert_eq!(out["comps"]["label"]["layers"][0]["text"], "Hi");
    let out = on_instances("scene comps.label.layers[1].fill=#0000FF")
        .unwrap()
        .into_raw();
    assert_eq!(out["comps"]["label"]["layers"][1]["fill"], "#0000FF");
    let out = on_instances("scene comps.label.layers[badge].fx[tint]=none")
        .unwrap()
        .into_raw();
    assert_eq!(out["comps"]["label"]["layers"][1]["fx"], json!([]));
    let err = on_instances("scene comps.label.layers[ghost].text=Hi").unwrap_err();
    assert_eq!(err.kind, Kind::UnknownName);
    assert_eq!(
        err.message,
        "cannot set `comps.label.layers[ghost].text`: `comps.label.layers` has no element `[ghost]`"
    );
}

#[test]
fn an_unknown_inner_layer_lists_every_address() {
    let err = on_instances("second/ghost text=x").unwrap_err();
    assert_eq!(err.message, "unknown layer `second/ghost`");
    assert!(
        err.expected
            .as_deref()
            .unwrap()
            .contains("deep/label/title"),
        "{err}"
    );
}

#[test]
fn an_invalid_inner_result_points_at_the_inner_layer() {
    let err = on_instances("second/title fill=red").unwrap_err();
    assert_eq!(err.path_text().as_deref(), Some("second/title.fill"));
}
