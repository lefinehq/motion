//! Applying a parsed [`Edit`] to a raw scene document.
//!
//! A side branch working on the raw document from step 1 (not a numbered rendering
//! step). It works on the raw JSON (not the
//! typed scene) so that untouched parts, and the order of keys, stay exactly
//! as the author wrote them. The result is re-validated with
//! `motion_format::scene_from_value` before it is returned.

use crate::{Edit, EditError, EditValue};
use motion_format::{FormatError, LAYER_KEYS, SCENE_KEYS, scene_from_value};
use serde_json::{Map, Value};

/// The reserved target that means the top level of the document.
const SCENE_TARGET: &str = "scene";

/// Applies `edit` to a clone of `raw` and returns the validated result.
///
/// The target `scene` always means the top level of the document, so a
/// layer whose id is `scene` cannot be edited this way.
/// Setting an existing key keeps its position; a new key is appended;
/// removing a key that is not there does nothing. `raw` is never changed.
///
/// # Errors
/// Returns an [`EditError`] for a key that is not allowed for the target,
/// an unknown layer id, a document without a `layers` array, or a result
/// that no longer passes validation (`invalid result: ...`).
pub fn apply_edit(raw: &Value, edit: &Edit) -> Result<Value, EditError> {
    let is_scene = edit.target == SCENE_TARGET;
    // Keys are checked first so a typo is reported even for a bad target.
    check_keys(edit, is_scene)?;
    let mut out = raw.clone();
    let object = if is_scene {
        out.as_object_mut()
            .ok_or_else(|| EditError::new("the document is not an object"))?
    } else {
        find_layer(&mut out, &edit.target)?
    };
    for change in &edit.changes {
        match &change.value {
            EditValue::Set(v) => {
                // `insert` replaces in place when the key exists (order is
                // preserved by serde_json's `preserve_order`), else appends.
                object.insert(change.key.clone(), v.clone());
            }
            EditValue::Remove => {
                object.shift_remove(&change.key);
            }
        }
    }
    scene_from_value(&out)
        .map_err(|e| EditError::new(format!("invalid result: {e}{}", quote_hint(edit, &e))))?;
    Ok(out)
}

/// When a bare number or boolean landed on a key that needs a string, says
/// how to write it: `; quote it: text="2024"`. Empty for every other error.
///
/// The edit grammar turns `text=2024` into a JSON number, which the format
/// then rejects as "must be a string"; the author almost always meant text.
fn quote_hint(edit: &Edit, error: &FormatError) -> String {
    if !error.is_not_a_string() {
        return String::new();
    }
    edit.changes
        .iter()
        .find_map(|c| match &c.value {
            // The error path is the bare key (scene) or ends in `.key` (layer).
            EditValue::Set(v @ (Value::Number(_) | Value::Bool(_)))
                if error.path == c.key || error.path.ends_with(&format!(".{}", c.key)) =>
            {
                Some(format!("; quote it: {}=\"{v}\"", c.key))
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Rejects keys that are not part of the format for this kind of target.
fn check_keys(edit: &Edit, is_scene: bool) -> Result<(), EditError> {
    let (allowed, what): (&[&str], &str) = if is_scene {
        (&SCENE_KEYS, "the scene")
    } else {
        (&LAYER_KEYS, "a layer")
    };
    for change in &edit.changes {
        if !allowed.contains(&change.key.as_str()) {
            return Err(EditError::new(format!(
                "unknown key `{}` for {what}; allowed: {}",
                change.key,
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

/// Finds the layer object with the given id.
fn find_layer<'a>(doc: &'a mut Value, id: &str) -> Result<&'a mut Map<String, Value>, EditError> {
    let layers = doc
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            EditError::new(format!(
                "the scene has no `layers` array, so there is no layer `{id}`"
            ))
        })?;
    layers
        .iter_mut()
        .filter_map(Value::as_object_mut)
        .find(|l| l.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| EditError::new(format!("no layer with id `{id}`")))
}

#[cfg(test)]
#[path = "../tests/unit/apply.rs"]
mod tests;
