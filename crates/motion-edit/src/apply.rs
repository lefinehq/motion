//! Applying a parsed [`Edit`] to a raw scene document.
//!
//! A side branch working on the raw document from step 1 (not a numbered rendering
//! step). It works on the raw JSON (not the
//! typed scene) so that untouched parts, and the order of keys, stay exactly
//! as the author wrote them. The result is re-validated with
//! `motion_format::scene_from_value` before it is returned.

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_format::{LAYER_KEYS, SCENE_KEYS, is_not_a_string, scene_from_value};
use serde_json::{Map, Value};

use crate::{Edit, EditValue};

/// The reserved target that means the top level of the document.
const SCENE_TARGET: &str = "scene";

/// Applies `edit` to a clone of `raw` and returns the validated result.
///
/// The target `scene` always means the top level of the document, so a
/// layer whose id is `scene` cannot be edited this way.
/// Setting an existing key keeps its position; a new key is appended;
/// removing a key that is not there does nothing; removing the last key of
/// a nested object removes the object too. `raw` is never changed.
///
/// # Errors
/// Returns a [`Diagnostic`] for a key (its first path segment) that is not
/// allowed for the target, a path through a value that is not an object,
/// an unknown layer id, a document without a `layers` array, or a result
/// that no longer passes validation (`invalid result: ...`, with the kind of
/// the validation failure).
pub fn apply_edit(raw: &Value, edit: &Edit) -> Result<Value> {
    let is_scene = edit.target == SCENE_TARGET;
    // Keys are checked first so a typo is reported even for a bad target.
    check_keys(edit, is_scene)?;
    let mut out = raw.clone();
    let object = if is_scene {
        out.as_object_mut()
            .ok_or_else(|| Diagnostic::new(Kind::Invalid, "the document is not an object"))?
    } else {
        find_layer(&mut out, &edit.target)?
    };
    for change in &edit.changes {
        match &change.value {
            EditValue::Set(v) => set_path(object, &change.key, v.clone())?,
            EditValue::Remove => {
                let segments: Vec<&str> = change.key.split('.').collect();
                remove_path(object, &segments);
            }
        }
    }
    // The whole validation message goes into this one; its kind is kept.
    scene_from_value(&out).map_err(|e| {
        Diagnostic::new(
            e.kind,
            format!("invalid result: {e}{}", quote_hint(edit, &e)),
        )
    })?;
    Ok(out)
}

/// When a bare number or boolean landed on a key that needs a string, says
/// how to write it: `; quote it: text="2024"`. Empty for every other error.
///
/// The edit grammar turns `text=2024` into a JSON number, which the format
/// then rejects as "must be a string"; the author almost always meant text.
fn quote_hint(edit: &Edit, error: &Diagnostic) -> String {
    if !is_not_a_string(error) {
        return String::new();
    }
    let path = error.path.as_deref().unwrap_or_default();
    edit.changes
        .iter()
        .find_map(|c| match &c.value {
            // The error path is the bare key (scene) or ends in `.key` (layer).
            EditValue::Set(v @ (Value::Number(_) | Value::Bool(_)))
                if path == c.key || path.ends_with(&format!(".{}", c.key)) =>
            {
                Some(format!("; quote it: {}=\"{v}\"", c.key))
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Sets a dotted path, creating the objects it needs. Setting an existing key
/// keeps its position (serde_json's `preserve_order`); a new key is appended.
fn set_path(root: &mut Map<String, Value>, key: &str, value: Value) -> Result<()> {
    let segments: Vec<&str> = key.split('.').collect();
    let (leaf, parents) = segments.split_last().expect("split yields one part");
    let mut object = root;
    for (i, seg) in parents.iter().enumerate() {
        let entry = object
            .entry(seg.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        object = entry.as_object_mut().ok_or_else(|| {
            Diagnostic::new(
                Kind::Invalid,
                format!(
                    "cannot set `{key}`: `{}` is not an object",
                    segments[..=i].join(".")
                ),
            )
        })?;
    }
    object.insert((*leaf).to_string(), value);
    Ok(())
}

/// Removes the leaf of a path, then any object that this left empty on the
/// way back up. Returns whether something was removed; a missing path (or a
/// non-object on the way) changes nothing. `object` itself is never removed.
fn remove_path(object: &mut Map<String, Value>, segments: &[&str]) -> bool {
    let [first, rest @ ..] = segments else {
        return false;
    };
    if rest.is_empty() {
        return object.shift_remove(*first).is_some();
    }
    let Some(Value::Object(child)) = object.get_mut(*first) else {
        return false;
    };
    let removed = remove_path(child, rest);
    if removed && child.is_empty() {
        object.shift_remove(*first);
    }
    removed
}

/// Rejects keys that are not part of the format for this kind of target.
fn check_keys(edit: &Edit, is_scene: bool) -> Result<()> {
    let (allowed, what): (&[&str], &str) = if is_scene {
        (&SCENE_KEYS, "the scene")
    } else {
        (&LAYER_KEYS, "a layer")
    };
    for change in &edit.changes {
        // Only the first segment is checked here; deeper ones are checked by
        // `scene_from_value` on the result.
        let first = change.key.split('.').next().unwrap_or_default();
        if !allowed.contains(&first) {
            return Err(Diagnostic::new(
                Kind::UnknownName,
                format!(
                    "unknown key `{first}` for {what}; allowed: {}",
                    allowed.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

/// Finds the layer object with the given id.
fn find_layer<'a>(doc: &'a mut Value, id: &str) -> Result<&'a mut Map<String, Value>> {
    let layers = doc
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!("the scene has no `layers` array, so there is no layer `{id}`"),
            )
        })?;
    layers
        .iter_mut()
        .filter_map(Value::as_object_mut)
        .find(|l| l.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| Diagnostic::new(Kind::UnknownName, format!("no layer with id `{id}`")))
}

#[cfg(test)]
#[path = "../tests/unit/apply.rs"]
mod tests;
