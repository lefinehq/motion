//! The canonical writer for scene documents: small diffs, one layer per line.

use serde_json::{Map, Value};

use crate::key::{COMPS, LAYERS};

/// Writes a document in canonical form: every other key minified on the
/// first line, then each composition in `comps` on lines of its own the same
/// way, then `layers`, one layer per line. Any other value is minified.
pub fn write_document(doc: &Value) -> String {
    match canvas(doc) {
        Some(text) => format!("{text}\n"),
        None => format!("{doc}\n"),
    }
}

/// An object with a `layers` array, written canonically; `None` for any other value.
fn canvas(value: &Value) -> Option<String> {
    let obj = value.as_object()?;
    let layers = obj.get(LAYERS)?.as_array()?;
    // Compositions get lines of their own only when every one is a canvas too.
    let comps: Option<Vec<String>> = obj.get(COMPS).and_then(Value::as_object).and_then(|comps| {
        comps
            .iter()
            .map(|(id, comp)| {
                canvas(comp).map(|text| format!("{}:{text}", Value::from(id.as_str())))
            })
            .collect()
    });
    // The other keys as one object without its closing brace, so the rest can follow.
    let rest: Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| *k != LAYERS && (comps.is_none() || *k != COMPS))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut text = match Value::Object(rest).to_string() {
        empty if empty == "{}" => "{".to_owned(),
        head => format!("{},", &head[..head.len() - 1]),
    };
    if let Some(comps) = comps.filter(|c| !c.is_empty()) {
        text.push_str(&format!("\n\"{COMPS}\":{{\n{}}},", comps.join(",\n")));
    }
    let mut body = layers
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(",\n");
    if !body.is_empty() {
        body.push('\n');
    }
    Some(format!("{text}\n\"{LAYERS}\":[\n{body}]}}"))
}

#[cfg(test)]
#[path = "../tests/unit/write.rs"]
mod tests;
