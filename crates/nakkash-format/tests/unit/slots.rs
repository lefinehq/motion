//! Unit tests for `src/slots.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::parse_document;

fn scene(extra: Json) -> Json {
    let mut scene = json!({"nakkash":1,"size":"200x100","dur":"1s","layers":[]});
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
#[case::colour_channels(json!([1, 0.5, 0, 1]), SlotType::Color, json!([1, 0.5, 0, 1]), None)]
#[case::text(json!({"text":"Launch"}), SlotType::Text, json!("Launch"), None)]
#[case::labelled(json!({"color":"#fff","label":"Accent"}), SlotType::Color, json!("#fff"), Some("Accent"))]
#[case::image(json!({"image":"brand/logo.png"}), SlotType::Image, json!("brand/logo.png"), None)]
#[case::no_image_yet(json!({"image":null,"label":"Logo"}), SlotType::Image, json!(null), Some("Logo"))]
#[case::video(json!({"video":"clips/hero.mov"}), SlotType::Video, json!("clips/hero.mov"), None)]
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
#[case::empty_file(json!({"image":""}))]
#[case::file_as_number(json!({"video":3}))]
#[case::role_as_number(json!({"color":"#fff","role":1}))]
fn a_slot_that_does_not_read_is_refused(#[case] written: Json) {
    assert_eq!(slot("accent", &written).unwrap_err().kind, Kind::Invalid);
}

#[test]
fn an_unknown_type_names_the_types() {
    let err = slot("accent", &json!({"colour":"#fff"})).unwrap_err();
    assert_eq!(
        err.expected.as_deref(),
        Some("one of number, point, color, text, image, video")
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

/// `card` shows its `title` and `accent` slots; the root's one layer draws
/// it with `overrides`.
fn card(overrides: Json) -> Json {
    scene(json!({
        "slots":{"promo":{"text":"Promo"}},
        "comps":{"card":{"size":"100x100","dur":"1s",
            "slots":{"title":{"text":"Hi"},"accent":"#FFFFFF"},
            "layers":[{"id":"t","text":"$title","fill":"$accent"}]}},
        "layers":[{"id":"a","comp":"card","overrides":overrides}]
    }))
}

/// The layers the root's first layer draws.
fn drawn(scene: &Json) -> Vec<crate::Layer> {
    let doc = parse_document(&scene.to_string()).unwrap();
    match &doc.scene().root.layers[0].content {
        crate::Content::Comp(comp) => comp.instance.clone().unwrap(),
        other => panic!("not a comp layer: {other:?}"),
    }
}

#[rstest]
#[case::own_value(json!({"$title":"Sale"}), json!({"t":{"text":"Sale"}}))]
#[case::two_values(json!({"$title":"Sale","$accent":"#E63946"}), json!({"t":{"text":"Sale","fill":"#E63946"}}))]
#[case::a_key_wins(json!({"$title":"Sale","t":{"text":"Exact"}}), json!({"t":{"text":"Exact"}}))]
#[case::the_scenes_slot(json!({"$title":"$promo"}), json!({"t":{"text":"Promo"}}))]
fn an_instance_gives_its_compositions_slots(#[case] given: Json, #[case] same_as: Json) {
    assert_eq!(drawn(&card(given)), drawn(&card(same_as)));
}

#[test]
fn an_instance_gives_slots_through_the_compositions_it_draws() {
    let nested = |overrides: Json| {
        scene(json!({
            "comps":{
                "card":{"size":"10x10","dur":"1s","slots":{"title":{"text":"Hi"}},
                    "layers":[{"id":"t","text":"$title"}]},
                "outer":{"size":"10x10","dur":"1s","slots":{"label":{"text":"Outer"}},
                    "layers":[{"id":"inner","comp":"card","overrides":{"$title":"$label"}}]}
            },
            "layers":[{"id":"o","comp":"outer","overrides":overrides}]
        }))
    };
    let text = |overrides: Json| {
        let crate::Content::Comp(inner) = &drawn(&nested(overrides))[0].content else {
            panic!("not a comp layer");
        };
        format!("{:?}", inner.instance.as_ref().unwrap()[0].content)
    };
    assert!(text(json!({"$label":"Nested"})).contains("Nested"));
    assert!(text(json!({"inner":{"opacity":1}})).contains("Outer"));
}

#[rstest]
#[case::no_such_slot(json!({"$ghost":"x"}), Kind::UnknownName, "a.overrides.$ghost")]
#[case::wrong_type(json!({"$accent":"Launch"}), Kind::Invalid, "a.overrides.$accent")]
#[case::not_keys(json!({"t":"Sale"}), Kind::Invalid, "a.overrides.t")]
fn an_instance_value_that_does_not_fit_is_placed_at_its_entry(
    #[case] given: Json,
    #[case] kind: Kind,
    #[case] place: &str,
) {
    let err = error(&card(given));
    assert_eq!((err.kind, err.path_text().as_deref()), (kind, Some(place)));
}

#[test]
fn a_listed_slot_has_the_values_that_follow_it_and_no_others() {
    let doc = parse_document(
        &scene(json!({
            "slots":{"accent":"#E63946","unused":1},
            "comps":{"card":{"size":"10x10","dur":"1s","slots":{"accent":"#00FF00"},
                "layers":[{"id":"r","rect":"1x1","fill":"$accent"}]}},
            "layers":[{"id":"p","rect":"1x1","fill":"$accent"},{"id":"c","comp":"card"}]
        }))
        .to_string(),
    )
    .unwrap();
    let slots = doc.slots();
    let listed: Vec<(Option<&str>, &str, Vec<&str>)> = slots
        .iter()
        .map(|l| {
            let followers = l.followers.iter().map(String::as_str).collect();
            (l.comp.as_deref(), &*l.slot.id, followers)
        })
        .collect();
    assert_eq!(
        listed,
        [
            (None, "accent", vec!["p.fill"]),
            (None, "unused", vec![]),
            (Some("card"), "accent", vec!["comps.card.layers[r].fill"]),
        ]
    );
}

#[test]
fn a_slot_may_say_what_it_is_for() {
    let slot = slot(
        "accent",
        &json!({"color":"#fff","label":"Accent","role":"accent"}),
    )
    .unwrap();
    assert_eq!(slot.role.as_deref(), Some("accent"));
    let err = slot_error(&json!({"color":"#fff","role":"Not An Id"}));
    assert_eq!(err.path_text().as_deref(), Some("role"));
}

fn slot_error(written: &Json) -> Diagnostic {
    slot("accent", written).unwrap_err()
}

/// The fill of the root's first layer, as the scene reads it.
fn fill(extra: Json) -> [f64; 4] {
    let doc = parse_document(&scene(extra).to_string()).unwrap();
    let resolved = resolve(doc.raw(), &declared(doc.raw()).unwrap()).unwrap().0;
    crate::color(&resolved["layers"][0]["fill"]).unwrap()
}

#[test]
fn a_reference_may_change_its_colour() {
    let red = [0.9, 0.2, 0.25, 1.0];
    let darker = crate::oklch::Shift::between(red, [0.7, 0.1, 0.15, 0.5]);
    let written = format!("$accent {darker}");
    let got = fill(json!({
        "slots":{"accent":red},
        "layers":[{"id":"r","rect":"10x10","fill":written}]
    }));
    assert!(
        got.iter()
            .zip([0.7, 0.1, 0.15, 0.5])
            .all(|(a, b)| (a - b).abs() < 2e-6),
        "{got:?}"
    );
    let half = fill(json!({
        "slots":{"accent":red},
        "layers":[{"id":"r","rect":"10x10","fill":"$accent / 0.5"}]
    }));
    assert_eq!(half, [0.9, 0.2, 0.25, 0.5]);
}

#[test]
fn a_change_of_colour_needs_a_colour_slot() {
    let err = error(&scene(json!({
        "slots":{"size":20},
        "layers":[{"id":"r","rect":"10x10","fill":"$size l+0.1"}]
    })));
    assert_eq!(err.path_text().as_deref(), Some("r.fill"));
    assert!(
        err.message.contains("takes no change of colour"),
        "{}",
        err.message
    );
}

#[rstest]
#[case::words("$title rocks")]
#[case::price("$99 off")]
fn text_after_an_id_that_is_no_change_stays(#[case] written: &str) {
    let doc = parse_document(
        &scene(json!({"slots":{"title":{"text":"x"}},"layers":[{"id":"t","text":written}]}))
            .to_string(),
    )
    .unwrap();
    match &doc.scene().root.layers[0].content {
        crate::Content::Text(content) => assert_eq!(content.text, written),
        other => panic!("{other:?}"),
    }
    assert!(!is_reference(written));
}

#[rstest]
#[case("$title", true)]
#[case("$accent l+0.1", true)]
#[case("$$99", true)]
#[case("$99", false)]
#[case("Sale", false)]
fn what_reads_as_a_reference(#[case] text: &str, #[case] reference: bool) {
    assert_eq!(is_reference(text), reference);
}

#[test]
fn the_length_and_background_may_follow_slots() {
    let doc = parse_document(
        &json!({"nakkash":1,"size":"200x100","dur":"$duration","bg":"$background",
            "slots":{"duration":{"number":2.5,"role":"duration"},"background":"#102030","card":"#FFFFFF"},
            "comps":{"card":{"size":"10x10","dur":"$duration","bg":"$card / 0.5","layers":[]}},
            "layers":[{"id":"c","comp":"card"}]})
        .to_string(),
    )
    .unwrap();
    assert_eq!(doc.scene().root.duration, crate::units::Time::Seconds(2.5));
    assert_eq!(
        doc.scene().comps[0].duration,
        crate::units::Time::Seconds(2.5)
    );
    let listed = doc.slots();
    let duration = listed.iter().find(|l| &*l.slot.id == "duration").unwrap();
    assert_eq!(duration.followers, ["dur", "comps.card.dur"]);
    let background = listed.iter().find(|l| &*l.slot.id == "background").unwrap();
    assert_eq!(background.followers, ["bg"]);
}
