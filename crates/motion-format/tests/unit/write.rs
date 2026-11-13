//! Unit tests for `src/write.rs`.

use super::*;

fn v(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn writes_one_layer_per_line_with_layers_last() {
    let doc = v(r#"{"layers":[{"id":"a","rect":"1x1"}],"motion":1,"dur":"1s"}"#);
    assert_eq!(
        write_document(&doc),
        "{\"motion\":1,\"dur\":\"1s\",\n\"layers\":[\n{\"id\":\"a\",\"rect\":\"1x1\"}\n]}\n"
    );
    let empty = v(r#"{"motion":1,"dur":"1s","layers":[]}"#);
    assert_eq!(
        write_document(&empty),
        "{\"motion\":1,\"dur\":\"1s\",\n\"layers\":[\n]}\n"
    );
}

#[test]
fn joins_layers_with_commas_and_newlines() {
    let doc = v(r#"{"motion":1,"layers":[{"id":"a"},{"id":"b"}]}"#);
    assert_eq!(
        write_document(&doc),
        "{\"motion\":1,\n\"layers\":[\n{\"id\":\"a\"},\n{\"id\":\"b\"}\n]}\n"
    );
}

#[test]
fn non_scene_values_are_written_minified() {
    assert_eq!(write_document(&v(r#"{ "a": 1 }"#)), "{\"a\":1}\n");
    assert_eq!(write_document(&v("[1, 2]")), "[1,2]\n");
}

#[test]
fn output_is_stable_when_written_twice() {
    let doc = v(r#"{"layers":[{"id":"a"}],"motion":1}"#);
    let once = write_document(&doc);
    let again = write_document(&serde_json::from_str(&once).unwrap());
    assert_eq!(once, again);
}
