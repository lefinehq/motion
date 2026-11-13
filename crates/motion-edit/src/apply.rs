//! Applying a parsed [`Edit`] to a raw scene document.
//!
//! A side branch working on the raw document from step 1 (not a numbered rendering
//! step). It works on the raw JSON (not the
//! typed scene) so that untouched parts, and the order of keys, stay exactly
//! as the author wrote them. The result is re-validated with
//! `motion_format::scene_from_value` before it is returned.

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_format::{LAYER_KEYS, SCENE_KEYS, scene_from_value};
use serde_json::Value;

use crate::path;
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
/// that no longer passes validation. The last keeps the validation
/// failure's kind, path and expected form, with `invalid result: ` before
/// its message; when quoting a bare number or boolean would fix it, the
/// expected form shows the quoted change instead.
pub fn apply_edit(raw: &Value, edit: &Edit) -> Result<Value> {
    let (out, target) = apply_changes(raw, edit)?;
    match scene_from_value(&out) {
        Ok(_) => Ok(out),
        Err(mut e) => {
            if let Some(quoted) = quote_hint(raw, edit, &target, &e) {
                e.expected = Some(quoted.into());
            }
            e.message = format!("invalid result: {}", e.message);
            Err(e)
        }
    }
}

/// Applies the changes of `edit` to a clone of `raw`, without validating
/// the result. Also returns the path of the edited object: empty for the
/// scene, `layers[i]` for a layer.
fn apply_changes(raw: &Value, edit: &Edit) -> Result<(Value, String)> {
    // Keys are checked first so a typo is reported even for a bad target.
    check_keys(edit)?;
    let mut out = raw.clone();
    let (target, object) = find_target(&mut out, &edit.target)?;
    for change in &edit.changes {
        match &change.value {
            EditValue::Set(value) => path::set(object, &change.path(), value.clone())?,
            // Removing a key that is not there is not an error: the result is
            // what was asked for.
            EditValue::Remove => _ = path::remove(object, &change.path()),
        }
    }
    Ok((out, target))
}

/// When a bare number or boolean landed on a key that needs a string, the
/// quoted change to write instead, e.g. ``a quoted value: `text="2024"` ``.
///
/// The edit grammar turns `text=2024` into a JSON number, which the format
/// rejects; the author almost always meant text. The decision is made on
/// structure, not on message text: `error` is a [`Kind::WrongType`] at
/// exactly the path a number or boolean change wrote, and the same edit with
/// that value as a JSON string no longer fails there at all. The
/// re-validation is what tells a string key from a number key: quoting
/// `fps=true` leaves `fps` just as wrong, so it gets no hint. Any error at
/// that path counts, not only a wrong type: quoting `dur=5` gives a time
/// without a unit, so the author is better served by the time's form than by
/// a quote. It runs only here, on the failure path, so a valid edit is
/// validated once.
fn quote_hint(raw: &Value, edit: &Edit, target: &str, error: &Diagnostic) -> Option<String> {
    if error.kind != Kind::WrongType {
        return None;
    }
    let (index, value) = edit
        .changes
        .iter()
        .enumerate()
        .find_map(|(i, c)| match &c.value {
            EditValue::Set(v @ (Value::Number(_) | Value::Bool(_)))
                if error.path.as_deref() == Some(key_path(target, &c.key).as_str()) =>
            {
                Some((i, v))
            }
            _ => None,
        })?;
    let key = &edit.changes[index].key;
    let mut quoted = edit.clone();
    quoted.changes[index].value = EditValue::Set(Value::String(value.to_string()));
    let (retried, _) = apply_changes(raw, &quoted).ok()?;
    let still_wrong = matches!(scene_from_value(&retried), Err(e) if e.path == error.path);
    (!still_wrong).then(|| format!("a quoted value: `{key}=\"{value}\"`"))
}

/// The path validation reports for `key` on the object at `target`.
fn key_path(target: &str, key: &str) -> String {
    if target.is_empty() {
        key.to_string()
    } else {
        format!("{target}.{key}")
    }
}

/// Rejects keys that are not part of the format for this kind of target.
/// Only the first segment of a path is checked here; deeper ones are checked
/// by `scene_from_value` on the result.
fn check_keys(edit: &Edit) -> Result<()> {
    let (allowed, what): (&[&str], &str) = if edit.target == SCENE_TARGET {
        (&SCENE_KEYS, "scene key")
    } else {
        (&LAYER_KEYS, "layer key")
    };
    edit.changes
        .iter()
        // A path always has a first segment, even when the key has no dot.
        .map(|change| change.path()[0])
        .find(|first| !allowed.contains(first))
        .map_or(Ok(()), |first| {
            Err(Diagnostic::unknown(what, first, allowed))
        })
}

/// Finds the object `target` names in `doc`, with the path validation
/// reports it under: the document itself (an empty path) for `scene`, or
/// the layer with that id (`layers[i]`).
fn find_target<'d>(doc: &'d mut Value, target: &str) -> Result<(String, &'d mut Value)> {
    if target == SCENE_TARGET {
        return if doc.is_object() {
            Ok((String::new(), doc))
        } else {
            Err(Diagnostic::new(
                Kind::WrongType,
                "the document is not an object",
            ))
        };
    }
    let layers = doc
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!("the scene has no `layers` array, so there is no layer `{target}`"),
            )
        })?;
    let index = layers
        .iter()
        .position(|l| l.get("id").and_then(Value::as_str) == Some(target))
        .ok_or_else(|| {
            let ids = layers.iter().filter_map(|l| l.get("id")?.as_str());
            Diagnostic::unknown("layer id", target, ids)
        })?;
    Ok((format!("layers[{index}]"), &mut layers[index]))
}

#[cfg(test)]
#[path = "../tests/unit/apply.rs"]
mod tests;
