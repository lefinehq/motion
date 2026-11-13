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
#[path = "../tests/unit/write.rs"]
mod tests;
