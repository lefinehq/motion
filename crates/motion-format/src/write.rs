//! The canonical writer for scene documents.
//!
//! Part of step 1 (Read), in reverse: it prints a document the way motion
//! wants it stored, so diffs stay small and a layer is one line.

use serde_json::Value;

/// Writes a document in canonical form.
///
/// Every key except `layers` is minified on the first line; `layers` always
/// comes last, one layer per line. A value that is not an object with a
/// `layers` array has no such shape, so it is just minified.
pub fn write_document(doc: &Value) -> String {
    let Some((obj, layers)) = doc
        .as_object()
        .and_then(|o| o.get("layers")?.as_array().map(|l| (o, l)))
    else {
        return format!("{doc}\n");
    };
    // Re-serialise the other keys as one object, then drop its closing brace
    // so the `layers` part can follow on the next line.
    let rest: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "layers")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut out = String::new();
    if !rest.is_empty() {
        let head = Value::Object(rest).to_string();
        out.push_str(&head[..head.len() - 1]);
        out.push_str(",\n");
    } else {
        out.push_str("{\n");
    }
    out.push_str("\"layers\":[\n");
    let lines: Vec<String> = layers.iter().map(Value::to_string).collect();
    out.push_str(&lines.join(",\n"));
    if !lines.is_empty() {
        out.push('\n');
    }
    out.push_str("]}\n");
    out
}

#[cfg(test)]
mod tests {
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
}
