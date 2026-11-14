//! Unit tests for `src/path.rs`.

use super::*;
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

#[test]
fn set_through_a_non_object_names_it() {
    let mut v = json!({"in": "fade", "a": {"b": 1}});
    for (key, parent) in [("in.opacity", "`in`"), ("a.b.c", "`a.b`")] {
        let e = set(&mut v, &at(key), json!(1)).unwrap_err();
        assert_eq!(e.kind, Kind::WrongType, "{key}");
        assert_eq!(e.path, None, "{key}");
        assert!(e.message.contains(parent), "{key}: {e}");
    }
}

#[test]
fn set_on_a_root_that_is_not_an_object_names_no_key() {
    let mut v = json!([1]);
    let e = set(&mut v, &at("in"), json!(1)).unwrap_err();
    assert_eq!(e.kind, Kind::WrongType);
    assert!(!e.message.contains("``"), "{e}");
}

#[test]
fn remove_cleans_up_the_objects_it_empties_but_not_the_root() {
    let mut v = json!({"a": {"b": {"c": 1}}});
    assert!(remove(&mut v, &at("a.b.c")));
    assert_eq!(v, json!({}));
}

#[test]
fn remove_keeps_parents_with_other_keys() {
    let mut v = json!({"anim": {"opacity": 1, "scale": 2}});
    assert!(remove(&mut v, &at("anim.opacity")));
    assert_eq!(v, json!({"anim": {"scale": 2}}));
}

#[test]
fn removing_a_missing_path_changes_nothing() {
    let mut v = json!({"a": {}, "in": "fade"});
    assert!(!remove(&mut v, &at("a.x")));
    assert!(!remove(&mut v, &at("in.x")));
    assert!(!remove(&mut v, &at("nope")));
    assert_eq!(v, json!({"a": {}, "in": "fade"}));
}
