//! Unit tests for `src/locks.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::{Value as Json, json};

use super::*;
use crate::parse_document;

fn scene(locks: Json) -> Json {
    json!({"nakkash":1,"size":"200x100","dur":"1s",
        "slots":{"accent":"#E63946"},
        "roles":{"logo":"logo","accent":"$accent"},
        "locks":locks,
        "layers":[{"id":"logo","rect":"10x10"},{"id":"title","text":"Hi"}]})
}

#[test]
fn locks_name_roles_layers_slots_and_the_named_locks() {
    let doc = parse_document(
        &scene(json!([
            "@logo",
            "title",
            "$accent",
            "palette",
            "typefaces",
            "@accent"
        ]))
        .to_string(),
    )
    .unwrap();
    let written: Vec<String> = doc.scene().locks.iter().map(ToString::to_string).collect();
    assert_eq!(
        written,
        [
            "@logo",
            "title",
            "$accent",
            "palette",
            "typefaces",
            "@accent"
        ]
    );
    let held: Vec<String> = doc
        .scene()
        .held_locks()
        .iter()
        .map(|(held, _)| held.to_string())
        .collect();
    assert_eq!(
        held,
        [
            "logo",
            "title",
            "$accent",
            "palette",
            "typefaces",
            "$accent"
        ]
    );
}

#[rstest]
#[case::no_role(json!(["@cta"]), Kind::UnknownName, "locks[0]")]
#[case::no_layer(json!(["title", "end"]), Kind::UnknownName, "locks[1]")]
#[case::no_slot(json!(["$ghost"]), Kind::UnknownName, "locks[0]")]
#[case::not_text(json!([3]), Kind::Invalid, "locks[0]")]
#[case::not_a_list(json!("palette"), Kind::Invalid, "locks")]
fn a_lock_on_nothing_is_refused_where_it_is(
    #[case] locks: Json,
    #[case] kind: Kind,
    #[case] place: &str,
) {
    let err = parse_document(&scene(locks).to_string()).unwrap_err();
    assert_eq!((err.kind, err.path_text().as_deref()), (kind, Some(place)));
}
