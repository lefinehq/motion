//! Applying a parsed [`Edit`] to a scene document.
//!
//! A side branch working on the document from step 1 (not a numbered
//! rendering step). Changes are made to the raw JSON, not the typed scene,
//! so that untouched parts, and the order of keys, stay exactly as the
//! author wrote them. The target is found in the typed scene, and the result
//! is re-validated with `motion_format::scene_from_value` before it is
//! returned.

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_format::key::LAYERS;
use motion_format::{Document, LAYER_KEYS, SCENE_KEYS, scene_from_value};
use serde_json::Value;
use tracing::{Span, field, instrument};

use crate::path;
use crate::{Edit, EditValue};

/// The reserved target that means the top level of the document.
const SCENE_TARGET: &str = "scene";

/// Applies `edit` to a copy of `doc` and returns the validated result: the
/// new raw JSON with the scene it describes.
///
/// The target `scene` always means the top level of the document, so a
/// layer whose id is `scene` cannot be edited this way.
/// Setting an existing key keeps its position; a new key is appended;
/// removing a key that is not there does nothing; removing the last key of
/// a nested object removes the object too. `doc` is never changed.
///
/// # Errors
/// Returns a [`Diagnostic`] for a key (its first path segment) that is not
/// allowed for the target, a path through a value that is not an object,
/// an unknown layer id, or a result that no longer passes validation. The
/// last is the validation failure itself; when quoting a bare number or
/// boolean would fix it, the expected form shows the quoted change instead.
///
/// # Panics
/// When `doc.raw` does not hold the layers of `doc.scene`, which a
/// [`Document`] from `motion_format::parse_document` always does.
#[instrument(
    level = "debug",
    skip_all,
    fields(target = %edit.target, keys = %change_keys(edit), outcome = field::Empty)
)]
pub fn apply_edit(doc: &Document, edit: &Edit) -> Result<Document> {
    let applied = validated(doc, edit);
    Span::current().record(
        "outcome",
        if applied.is_ok() {
            "applied"
        } else {
            "rejected"
        },
    );
    applied
}

/// The changed keys of `edit`, comma-separated, for the `apply_edit` span.
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
    /// Finds the object `name` stands for in `doc`: `scene`, or the layer
    /// with that id.
    fn find(doc: &Document, name: &str) -> Result<Target> {
        if name == SCENE_TARGET {
            return Ok(Target::Scene);
        }
        let layers = &doc.scene.layers;
        match layers.iter().position(|layer| layer.id == name) {
            Some(index) => Ok(Target::Layer(index)),
            None => {
                let ids = layers.iter().map(|layer| &layer.id);
                Err(Diagnostic::unknown("layer id", name, ids))
            }
        }
    }

    /// The target's object in `raw`, the JSON of the document it was found
    /// in. A parsed document's raw `layers` are its scene's layers, in the
    /// same order, so the index is all it takes.
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
            Target::Layer(index) => format!("{LAYERS}[{index}].{key}"),
        }
    }
}

/// [`apply_edit`] without the span.
fn validated(doc: &Document, edit: &Edit) -> Result<Document> {
    let (raw, target) = apply_changes(doc, edit)?;
    match scene_from_value(&raw) {
        Ok(scene) => Ok(Document { raw, scene }),
        Err(e) => Err(match quote_hint(doc, edit, target, &e) {
            Some(quoted) => e.expected(quoted),
            None => e,
        }),
    }
}

/// Applies the changes of `edit` to a copy of `doc`'s JSON, without
/// validating the result. Also returns the target the changes went to.
fn apply_changes(doc: &Document, edit: &Edit) -> Result<(Value, Target)> {
    // Keys are checked first so a typo is reported even for a bad target.
    check_keys(edit)?;
    let target = Target::find(doc, &edit.target)?;
    let mut raw = doc.raw.clone();
    let object = target.object(&mut raw);
    for change in &edit.changes {
        match &change.value {
            EditValue::Set(value) => path::set(object, &change.key, value.clone())?,
            // Removing a key that is not there is not an error: the result is
            // what was asked for.
            EditValue::Remove => _ = path::remove(object, &change.key),
        }
    }
    Ok((raw, target))
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
    let still_wrong = matches!(scene_from_value(&retried), Err(e) if e.path == error.path);
    (!still_wrong).then(|| format!("a quoted value: `{key}=\"{value}\"`"))
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
        .map(|change| change.key.first())
        .find(|first| !allowed.contains(first))
        .map_or(Ok(()), |first| {
            Err(Diagnostic::unknown(what, first, allowed))
        })
}

#[cfg(test)]
#[path = "../tests/unit/apply.rs"]
mod tests;
