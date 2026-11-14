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

#[test]
fn writes_each_composition_on_lines_of_its_own() {
    let doc = v(
        r#"{"motion":1,"comps":{"card":{"size":"1x1","layers":[{"id":"t"},{"id":"u"}]},"b":{"layers":[]}},"layers":[{"id":"a","comp":"card"}]}"#,
    );
    let written = write_document(&doc);
    assert_eq!(
        written,
        "{\"motion\":1,\n\"comps\":{\n\"card\":{\"size\":\"1x1\",\n\"layers\":[\n{\"id\":\"t\"},\n{\"id\":\"u\"}\n]},\n\"b\":{\n\"layers\":[\n]}},\n\"layers\":[\n{\"id\":\"a\",\"comp\":\"card\"}\n]}\n"
    );
    let read: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(read, doc);
    assert_eq!(write_document(&read), written);
}

#[test]
fn compositions_that_are_not_canvases_stay_on_the_first_line() {
    let doc = v(r#"{"motion":1,"comps":{"x":1},"layers":[]}"#);
    assert_eq!(
        write_document(&doc),
        "{\"motion\":1,\"comps\":{\"x\":1},\n\"layers\":[\n]}\n"
    );
}
