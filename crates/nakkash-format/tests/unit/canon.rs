use serde_json::json;

use super::*;

#[test]
fn key_order_and_spacing_leave_the_hash() {
    let a: Json = serde_json::from_str(r#"{"nakkash":1,"size":"1080p","layers":[{"id":"a","fill":"#fff"}]}"#)
        .expect("JSON");
    let b: Json = serde_json::from_str("{ \"layers\": [ {\"fill\": \"#fff\", \"id\": \"a\"} ],\n \"size\": \"1080p\", \"nakkash\": 1 }")
        .expect("JSON");
    assert_eq!(canonical(&a), canonical(&b));
    assert_eq!(hash(&a), hash(&b));
    assert!(hash(&a).starts_with("sha256:") && hash(&a).len() == 7 + 64);
}

#[test]
fn any_other_change_moves_the_hash() {
    let a = json!({"nakkash":1,"layers":[{"id":"a"},{"id":"b"}]});
    let reordered = json!({"nakkash":1,"layers":[{"id":"b"},{"id":"a"}]});
    let changed = json!({"nakkash":1,"layers":[{"id":"a"},{"id":"c"}]});
    assert_ne!(hash(&a), hash(&reordered), "list order means something");
    assert_ne!(hash(&a), hash(&changed));
}
