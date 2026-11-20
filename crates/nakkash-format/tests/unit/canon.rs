use serde_json::json;

use super::*;

#[test]
fn spacing_leaves_the_hash_and_key_order_moves_it() {
    let a: Json = serde_json::from_str(r#"{"nakkash":1,"size":"1080p","layers":[{"id":"a","fill":"#fff"}]}"#)
        .expect("JSON");
    let spaced: Json = serde_json::from_str("{ \"nakkash\": 1,\n \"size\": \"1080p\", \"layers\": [ {\"id\": \"a\", \"fill\": \"#fff\"} ] }")
        .expect("JSON");
    let reordered: Json = serde_json::from_str(r#"{"size":"1080p","nakkash":1,"layers":[{"fill":"#fff","id":"a"}]}"#)
        .expect("JSON");
    assert_eq!(canonical(&a), canonical(&spaced));
    assert_eq!(hash(&a), hash(&spaced));
    assert_ne!(hash(&a), hash(&reordered), "a layer keeps its values in the order written");
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
