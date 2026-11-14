//! The canonical writer for scene documents: small diffs, one layer per line.

use serde_json::Value;

use crate::key::LAYERS;

/// Writes a document in canonical form: every other key minified on the
/// first line, then `layers`, one layer per line. Any other value is minified.
pub fn write_document(doc: &Value) -> String {
    let Some((obj, layers)) = doc
        .as_object()
        .and_then(|o| o.get(LAYERS)?.as_array().map(|l| (o, l)))
    else {
        return format!("{doc}\n");
    };
    // The other keys as one object without its closing brace, so `layers` can follow.
    let rest: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != LAYERS)
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
    out.push_str(&format!("\"{LAYERS}\":[\n"));
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
