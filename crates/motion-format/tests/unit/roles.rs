//! Unit tests for `src/roles.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::{Value as Json, json};

use super::*;
use crate::parse_document;

/// A scene with a slot `accent`, a layer `title` and a `card` instance
/// drawing a composition whose layer is `label`, with `extra` added.
fn scene(extra: Json) -> Json {
    let mut scene = json!({"motion":1,"size":"200x100","dur":"1s",
        "slots":{"accent":"#E63946"},
        "comps":{"tag":{"size":"10x10","dur":"1s","layers":[{"id":"label","text":"Hi"}]}},
        "layers":[{"id":"title","text":"Launch"},{"id":"card","comp":"tag"}]});
    scene
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    scene
}

#[test]
fn a_role_names_a_layer_at_any_depth_or_a_slot() {
    let doc = parse_document(
        &scene(json!({"roles":{"headline":"title","tag":"card/label","accent":"$accent"}}))
            .to_string(),
    )
    .unwrap();
    let roles: Vec<String> = doc
        .scene()
        .roles
        .iter()
        .map(|r| format!("{} {}", r.id, r.target))
        .collect();
    assert_eq!(
        roles,
        ["headline title", "tag card/label", "accent $accent"]
    );
    assert_eq!(
        doc.scene().role("accent").map(|r| &r.target),
        Some(&RoleTarget::Slot("accent".parse().unwrap()))
    );
}

#[rstest]
#[case::no_layer(json!({"headline":"ghost"}), Kind::UnknownName, "roles.headline")]
#[case::no_slot(json!({"accent":"$ghost"}), Kind::UnknownName, "roles.accent")]
#[case::keys(json!({"headline":"title.text"}), Kind::UnknownName, "roles.headline")]
#[case::bad_name(json!({"Headline":"title"}), Kind::Invalid, "roles.Headline")]
#[case::not_text(json!({"headline":3}), Kind::Invalid, "roles.headline")]
fn a_role_that_names_nothing_is_refused_at_its_name(
    #[case] roles: Json,
    #[case] kind: Kind,
    #[case] place: &str,
) {
    let err = parse_document(&scene(json!({ "roles": roles })).to_string()).unwrap_err();
    assert_eq!((err.kind, err.path_text().as_deref()), (kind, Some(place)));
}
