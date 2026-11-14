//! Unit tests for `src/path.rs`.

use super::*;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

/// A dotted path, as the edit grammar reads it.
fn at(path: &str) -> KeyPath {
    path.parse().unwrap()
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

#[test]
fn set_creates_the_objects_on_the_way() {
    let mut v = json!({"id": "card"});
    set(&mut v, &at("anim.opacity"), json!([[0, 1]])).unwrap();
    assert_eq!(v, json!({"id": "card", "anim": {"opacity": [[0, 1]]}}));
}

#[test]
fn set_keeps_an_existing_key_in_place_and_appends_a_new_one() {
    let mut v = json!({"a": 1, "b": 2});
    set(&mut v, &at("a"), json!(3)).unwrap();
    set(&mut v, &at("c"), json!(4)).unwrap();
    assert_eq!(keys(&v), ["a", "b", "c"]);
    assert_eq!(v["a"], 3);
}

#[rstest]
#[case("in.opacity", "`in`")]
#[case("a.b.c", "`a.b`")]
fn set_through_a_non_object_names_it(#[case] key: &str, #[case] parent: &str) {
    let mut v = json!({"in": "fade", "a": {"b": 1}});
    let e = set(&mut v, &at(key), json!(1)).unwrap_err();
    assert_eq!(e.kind, Kind::WrongType);
    assert_eq!(e.path, None);
    assert!(e.message.contains(parent), "{e}");
}

#[test]
fn set_on_a_root_that_is_not_an_object_names_no_key() {
    let mut v = json!([1]);
    let e = set(&mut v, &at("in"), json!(1)).unwrap_err();
    assert_eq!(e.kind, Kind::WrongType);
    assert!(!e.message.contains("``"), "{e}");
}

#[rstest]
#[case::cleans_up_the_objects_it_empties_but_not_the_root(
    json!({"a": {"b": {"c": 1}}}),
    "a.b.c",
    json!({})
)]
#[case::keeps_parents_with_other_keys(
    json!({"anim": {"opacity": 1, "scale": 2}}),
    "anim.opacity",
    json!({"anim": {"scale": 2}})
)]
fn remove_drops_the_key(#[case] mut v: Value, #[case] key: &str, #[case] after: Value) {
    assert!(remove(&mut v, &at(key)));
    assert_eq!(v, after);
}

#[test]
fn removing_a_missing_path_changes_nothing() {
    let mut v = json!({"a": {}, "in": "fade"});
    assert!(!remove(&mut v, &at("a.x")));
    assert!(!remove(&mut v, &at("in.x")));
    assert!(!remove(&mut v, &at("nope")));
    assert_eq!(v, json!({"a": {}, "in": "fade"}));
}
