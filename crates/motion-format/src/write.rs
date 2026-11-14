//! The canonical writer for scene documents: small diffs, one layer per line.

use serde_json::{Map, Value};

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
    let rest: Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| *k != LAYERS)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let head = match Value::Object(rest).to_string() {
        empty if empty == "{}" => "{".to_owned(),
        head => format!("{},", &head[..head.len() - 1]),
    };
    let mut body = layers
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(",\n");
    if !body.is_empty() {
        body.push('\n');
    }
    format!("{head}\n\"{LAYERS}\":[\n{body}]}}\n")
}

#[cfg(test)]
#[path = "../tests/unit/write.rs"]
mod tests;
