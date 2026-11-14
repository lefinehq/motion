//! Applying an [`Edit`] to a [`Document`]. Changes go to the raw JSON, so
//! untouched parts and the order of keys stay as the author wrote them.

use motion_format::key::{LAYERS, layer_path};
use motion_format::{Diagnostic, Kind, Result};
use motion_format::{Document, SCENE_KEYS, layer_keys};
use serde_json::Value;
use tracing::{Span, field, instrument};

use crate::path;
use crate::{Edit, EditValue};

/// The reserved target that means the top level of the document.
const SCENE_TARGET: &str = "scene";

/// Applies `edit` to a copy of `doc` and returns the re-validated result.
///
/// An existing key keeps its position and a new one is appended; removing a
/// missing key does nothing. The target `scene` always means the top level.
///
/// # Errors
/// A diagnostic for a key that does not fit the target, an unknown layer id,
/// a path through a non-object, or an invalid result; when quoting a bare
/// value would fix the result, its expected form shows the quoted change.
#[instrument(
    level = "debug",
    skip_all,
    fields(target = %edit.target, keys = %change_keys(edit), outcome = field::Empty)
)]
pub fn apply_edit(doc: &Document, edit: &Edit) -> Result<Document> {
    let applied = validated(doc, edit);
    let outcome = if applied.is_ok() {
        "applied"
    } else {
        "rejected"
    };
    Span::current().record("outcome", outcome);
    applied
}

/// The changed keys of `edit`, comma-separated, for the span.
fn change_keys(edit: &Edit) -> String {
    let keys: Vec<&str> = edit.changes.iter().map(|c| c.key.as_str()).collect();
    keys.join(",")
}

/// What an edit changes: the top level of the document, or one layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// The document itself, written `scene`.
    Scene,
    /// The layer at this index, in the scene and in the raw `layers` array.
    Layer(usize),
}

impl Target {
    /// The target `name` stands for in `doc`: `scene`, or a layer's id.
    fn find(doc: &Document, name: &str) -> Result<Target> {
        if name == SCENE_TARGET {
            return Ok(Target::Scene);
        }
        let layers = &doc.scene().layers;
        layers
            .iter()
            .position(|layer| layer.id == name)
            .map(Target::Layer)
            .ok_or_else(|| {
                Diagnostic::unknown("layer id", name, layers.iter().map(|layer| &layer.id))
            })
    }

    /// The target's object in `raw`, whose `layers` match the scene's, in order.
    fn object(self, raw: &mut Value) -> &mut Value {
        match self {
            Target::Scene => raw,
            Target::Layer(index) => &mut raw[LAYERS][index],
        }
    }

    /// The path validation reports for `key` on the target.
    fn path_of(self, key: &str) -> String {
        match self {
            Target::Scene => key.to_owned(),
            Target::Layer(index) => format!("{}.{key}", layer_path(index)),
        }
    }
}

/// [`apply_edit`] without the span.
fn validated(doc: &Document, edit: &Edit) -> Result<Document> {
    let (raw, target) = apply_changes(doc, edit)?;
    Document::from_value(raw).map_err(|e| match quote_hint(doc, edit, target, &e) {
        Some(quoted) => e.expected(quoted),
        None => e,
    })
}

/// `edit` applied to a copy of `doc`'s JSON, unvalidated, and its target.
fn apply_changes(doc: &Document, edit: &Edit) -> Result<(Value, Target)> {
    // Keys first, so a typo is reported even for a bad target.
    check_keys(edit)?;
    let target = Target::find(doc, &edit.target)?;
    let mut raw = doc.raw().clone();
    let object = target.object(&mut raw);
    for change in &edit.changes {
        match &change.value {
            EditValue::Set(value) => path::set(object, &change.key, value.clone())?,
            // Removing a missing key already gives what was asked for.
            EditValue::Remove => _ = path::remove(object, &change.key),
        }
    }
    Ok((raw, target))
}

/// The quoted change to suggest when a bare number or boolean landed on a
/// key that needs a string: ``a quoted value: `text="2024"` ``.
///
/// It is decided on structure, not message text: `error` is a wrong type at
/// the path the change wrote, and the quoted edit no longer fails there at
/// all (quoting `fps=true` or `dur=5` would, so they get the key's own form).
fn quote_hint(doc: &Document, edit: &Edit, target: Target, error: &Diagnostic) -> Option<String> {
    if error.kind != Kind::WrongType {
        return None;
    }
    let (index, value) = edit
        .changes
        .iter()
        .enumerate()
        .find_map(|(i, c)| match &c.value {
            EditValue::Set(v @ (Value::Number(_) | Value::Bool(_)))
                if error.path.as_deref() == Some(target.path_of(c.key.as_str()).as_str()) =>
            {
                Some((i, v))
            }
            _ => None,
        })?;
    let key = &edit.changes[index].key;
    let mut quoted = edit.clone();
    quoted.changes[index].value = EditValue::Set(Value::String(value.to_string()));
    let (retried, _) = apply_changes(doc, &quoted).ok()?;
    let still_wrong = matches!(Document::from_value(retried), Err(e) if e.path == error.path);
    (!still_wrong).then(|| format!("a quoted value: `{key}=\"{value}\"`"))
}

/// Rejects a key whose first segment is not a key of the target's kind;
/// deeper segments are left to validation.
fn check_keys(edit: &Edit) -> Result<()> {
    let (allowed, what): (Vec<&str>, &str) = if edit.target == SCENE_TARGET {
        (SCENE_KEYS.to_vec(), "scene key")
    } else {
        (layer_keys().collect(), "layer key")
    };
    match edit
        .changes
        .iter()
        .map(|change| change.key.first())
        .find(|first| !allowed.contains(first))
    {
        Some(first) => Err(Diagnostic::unknown(what, first, allowed)),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "../tests/unit/apply.rs"]
mod tests;
