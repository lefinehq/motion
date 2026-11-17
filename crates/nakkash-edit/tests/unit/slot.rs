//! Unit tests for `src/slot.rs`.

use nakkash_format::Kind;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::{Value, json};

use crate::{apply_edit, parse_edit};

/// A scene with slots of each form, a composition `card` with its own, and
/// `outer`, which draws `card` and passes its `label` on.
fn scene() -> Value {
    json!({"nakkash":1,"size":"200x100","dur":"1s",
        "slots":{"accent":{"color":"#E63946","label":"Accent"},"lift":40,"unused":"#000"},
        "comps":{
            "card":{"size":"10x10","dur":"1s","slots":{"title":{"text":"Hi"}},
                "layers":[{"id":"t","text":"$title"}]},
            "outer":{"size":"10x10","dur":"1s","slots":{"label":{"text":"Outer"}},
                "layers":[{"id":"inner","comp":"card","overrides":{"$title":"$label"}}]}},
        "layers":[
            {"id":"plate","rect":"10x10","fill":"$accent","rotate":"$lift"},
            {"id":"card-2","comp":"card"},
            {"id":"o","comp":"outer"}]})
}

fn edit(line: &str) -> nakkash_format::Result<Value> {
    let doc = nakkash_format::Document::from_value(scene())?;
    apply_edit(&doc, &parse_edit(line)?).map(nakkash_format::Document::into_raw)
}

#[rstest]
#[case::keeps_its_label("$accent=#2A9D8F", "accent", json!({"color":"#2A9D8F","label":"Accent"}))]
#[case::keeps_its_shorthand("$lift=60", "lift", json!(60))]
#[case::written_out_for_another_type("$unused=0.5", "unused", json!({"color":0.5}))]
#[case::with_its_target("scene $lift=10", "lift", json!(10))]
fn a_scene_slot_takes_the_new_value_in_its_form(
    #[case] line: &str,
    #[case] id: &str,
    #[case] written: Value,
) {
    match edit(line) {
        Ok(raw) => assert_eq!(raw["slots"][id], written),
        // A value that does not fit stays the slot's type, which the check names.
        Err(e) => assert_eq!(
            (e.kind, e.message.as_str()),
            (Kind::Invalid, "`0.5` for a color slot")
        ),
    }
}

#[test]
fn an_instance_gets_its_own_value() {
    let raw = edit("card-2 $title=Sale").unwrap();
    assert_eq!(raw["layers"][1]["overrides"], json!({"$title":"Sale"}));
    let raw = edit("o/inner $title=Deep").unwrap();
    assert_eq!(
        raw["layers"][2]["overrides"],
        json!({"inner":{"overrides":{"$title":"Deep"}}})
    );
}

#[test]
fn removing_an_instances_value_leaves_no_overrides() {
    let doc = nakkash_format::Document::from_value(scene()).unwrap();
    let doc = apply_edit(&doc, &parse_edit("card-2 $title=Sale").unwrap()).unwrap();
    let raw = apply_edit(&doc, &parse_edit("card-2 $title=none").unwrap())
        .unwrap()
        .into_raw();
    assert_eq!(raw["layers"][1].get("overrides"), None);
}

#[test]
fn a_slot_goes_once_nothing_follows_it() {
    let raw = edit("$unused=none").unwrap();
    assert_eq!(raw["slots"].get("unused"), None);
    let err = edit("$accent=none").unwrap_err();
    assert_eq!(
        (err.kind, err.message.as_str()),
        (
            Kind::Conflict,
            "slot `$accent` cannot go while values follow it: plate.fill"
        )
    );
}

#[rstest]
#[case::undeclared("$ghost=1", Kind::UnknownName)]
#[case::undeclared_on_an_instance("card-2 $ghost=1", Kind::UnknownName)]
#[case::wrong_type_on_an_instance("card-2 $title=3", Kind::Invalid)]
#[case::not_a_comp_layer("plate $accent=#fff", Kind::Invalid)]
#[case::not_an_id("$Accent=#fff", Kind::Syntax)]
#[case::no_keys_inside("$accent.x=1", Kind::Syntax)]
fn a_slot_edit_that_cannot_be_made_is_refused(#[case] line: &str, #[case] kind: Kind) {
    assert_eq!(edit(line).unwrap_err().kind, kind, "{line}");
}
