//! Unit tests for `src/slots.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::parse_document;

fn scene(extra: Json) -> Json {
    let mut scene = json!({"motion":1,"size":"200x100","dur":"1s","layers":[]});
    scene
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    scene
}

fn error(scene: &Json) -> Diagnostic {
    parse_document(&scene.to_string()).unwrap_err()
}

#[rstest]
#[case::colour(json!("#E63946"), SlotType::Color, json!("#E63946"), None)]
#[case::number(json!(1.5), SlotType::Number, json!(1.5), None)]
#[case::point(json!([3, 4]), SlotType::Point, json!([3, 4]), None)]
#[case::text(json!({"text":"Launch"}), SlotType::Text, json!("Launch"), None)]
#[case::labelled(json!({"color":"#fff","label":"Accent"}), SlotType::Color, json!("#fff"), Some("Accent"))]
fn a_slot_is_typed_by_its_value(
    #[case] written: Json,
    #[case] ty: SlotType,
    #[case] value: Json,
    #[case] label: Option<&str>,
) {
    let slot = slot("accent", &written).unwrap();
    assert_eq!(
        (slot.ty, slot.value, slot.label.as_deref()),
        (ty, value, label)
    );
}

#[rstest]
#[case::not_a_colour(json!("Launch"))]
#[case::three_numbers(json!([1, 2, 3]))]
#[case::text_as_number(json!({"text":3}))]
#[case::two_types(json!({"color":"#fff","number":1}))]
#[case::null(json!(null))]
fn a_slot_that_does_not_read_is_refused(#[case] written: Json) {
    assert_eq!(slot("accent", &written).unwrap_err().kind, Kind::Invalid);
}

#[test]
fn an_unknown_type_names_the_types() {
    let err = slot("accent", &json!({"colour":"#fff"})).unwrap_err();
    assert_eq!(
        err.expected.as_deref(),
        Some("one of number, point, color, text")
    );
}

#[test]
fn a_bad_slot_is_placed_at_its_id() {
    let err = error(&scene(json!({"slots":{"accent":"red"}})));
    assert_eq!(err.path_text().as_deref(), Some("slots.accent"));
    let err = error(&scene(
        json!({"comps":{"card":{"slots":{"Bad":1},"layers":[]}}}),
    ));
    assert_eq!(err.path_text().as_deref(), Some("comps.card.slots.Bad"));
}

#[test]
fn values_follow_the_scenes_slots() {
    let doc = parse_document(
        &scene(json!({
            "slots":{"accent":"#E63946","title":{"text":"Launch"}},
            "layers":[{"id":"t","text":"$title","fill":"$accent"}]
        }))
        .to_string(),
    )
    .unwrap();
    let layer = &doc.scene().root.layers[0];
    match &layer.content {
        crate::Content::Text(text) => assert_eq!(text.text, "Launch"),
        other => panic!("{other:?}"),
    }
    // The document keeps the references as written.
    assert_eq!(doc.raw()["layers"][0]["fill"], json!("$accent"));
    assert_eq!(doc.scene().root.slots.len(), 2);
}

#[test]
fn a_composition_reads_its_own_slots_then_the_scenes() {
    let doc = parse_document(
        &scene(json!({
            "slots":{"accent":"#E63946","size":20},
            "comps":{"card":{"size":"100x100","dur":"1s","slots":{"accent":"#00FF00"},
                "layers":[{"id":"r","rect":"10x10","fill":"$accent","opacity":1}]}},
            "layers":[{"id":"c","comp":"card"},{"id":"r","rect":"10x10","fill":"$accent"}]
        }))
        .to_string(),
    )
    .unwrap();
    let card = &doc.scene().comps[0];
    assert_eq!(card.slots[0].value, json!("#00FF00"));
    assert_eq!(doc.scene().root.slots[0].value, json!("#E63946"));
}

#[rstest]
#[case::scene(json!({"layers":[{"id":"r","rect":"10x10","fill":"$accent"}]}), "r.fill")]
#[case::nested(json!({"slots":{"a":1},"layers":[{"id":"r","rect":"10x10","anim":{"x":[[0,"$b"]]}}]}), "r.anim.x[0][1]")]
fn a_reference_to_no_slot_is_reported_where_it_is(#[case] extra: Json, #[case] place: &str) {
    let err = error(&scene(extra));
    assert_eq!(err.kind, Kind::UnknownName);
    assert_eq!(err.path_text().as_deref(), Some(place));
}

#[test]
fn a_composition_cannot_see_another_compositions_slots() {
    let err = error(&scene(json!({
        "comps":{
            "a":{"size":"10x10","dur":"1s","slots":{"accent":"#fff"},"layers":[]},
            "b":{"size":"10x10","dur":"1s","layers":[{"id":"r","rect":"1x1","fill":"$accent"}]}
        }
    })));
    assert_eq!(err.message, "unknown slot `$accent`");
}

#[test]
fn a_value_that_does_not_fit_names_its_slot() {
    let err = error(&scene(json!({
        "slots":{"size":20},
        "layers":[{"id":"r","rect":"10x10","fill":"$size"}]
    })));
    assert_eq!(err.path_text().as_deref(), Some("r.fill"));
    assert!(
        err.message.ends_with("(the value of slot `$size`)"),
        "{}",
        err.message
    );
}

#[rstest]
#[case::escaped("$$99", "$99")]
#[case::not_an_id("$99", "$99")]
#[case::plain("Sale", "Sale")]
fn dollars_that_are_not_references_stay(#[case] written: &str, #[case] text: &str) {
    let doc =
        parse_document(&scene(json!({"layers":[{"id":"t","text":written}]})).to_string()).unwrap();
    match &doc.scene().root.layers[0].content {
        crate::Content::Text(content) => assert_eq!(content.text, text),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_scene_without_dollars_is_not_copied() {
    let raw = scene(json!({"layers":[{"id":"t","text":"hi"}]}));
    let (resolved, followed) = resolve(&raw, &Declared::default()).unwrap();
    assert!(matches!(resolved, Cow::Borrowed(_)));
    assert!(followed.followed.is_empty());
}
