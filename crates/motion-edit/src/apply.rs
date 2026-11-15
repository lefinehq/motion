//! Applying an [`Edit`] to a [`Document`]. Changes go to the raw JSON, so
//! untouched parts and the order of keys stay as the author wrote them.

use motion_format::address::Id;
use motion_format::key::{COMP, LAYERS, OVERRIDES};
use motion_format::{Address, Diagnostic, Kind, Place, Result};
use motion_format::{Document, SCENE_KEYS, layer_keys};
use serde_json::Map;
use serde_json::Value;
use tracing::{Span, field, instrument};

use crate::{Edit, EditValue, KeyPath};
use crate::{path, slot};

/// Applies `edit` to a copy of `doc` and returns the re-validated result.
///
/// An existing key keeps its position and a new one is appended; removing a
/// missing key does nothing. The target `scene` always means the top level.
/// A target inside a composition, `card/title`, is edited for that instance
/// alone: the change goes into `overrides` on the scene's layer the address
/// starts at, `card`.
///
/// # Errors
/// A diagnostic for a key that does not fit the target, an unknown layer, a
/// path through a non-object, a key an override cannot remove, or an
/// invalid result; when quoting a bare value would fix the result, its
/// expected form shows the quoted change.
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
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// The document itself, written `scene`.
    Scene,
    /// A layer of the scene's root, at this index of the raw `layers`.
    Layer(usize),
    /// A layer inside a composition, edited through `overrides` on the
    /// root layer at `index`: `inner` is its address from there, `title`
    /// of `card/title`.
    Inner {
        /// The root layer the address starts at.
        index: usize,
        /// The rest of the address, at least one id.
        inner: Vec<Id>,
    },
}

impl Target {
    /// The target `address` stands for in `doc`.
    fn find(doc: &Document, address: &Address) -> Result<Target> {
        let Some((first, inner)) = address.layers.split_first() else {
            return Ok(Target::Scene);
        };
        let scene = doc.scene();
        let index = scene
            .root
            .layers
            .iter()
            .position(|layer| layer.id == first.as_ref());
        match (index, scene.layer(&address.layers)) {
            (Some(index), Some(_)) if inner.is_empty() => Ok(Target::Layer(index)),
            (Some(index), Some(_)) => Ok(Target::Inner {
                index,
                inner: inner.to_vec(),
            }),
            _ => Err(Diagnostic::unknown("layer", address, scene.addresses())),
        }
    }

    /// The target's object in `raw`, whose `layers` match the scene's, in
    /// order: for an inner layer, its override on the root layer, made
    /// empty when there is none yet.
    fn object<'r>(&self, raw: &'r mut Value) -> &'r mut Value {
        match self {
            Target::Scene => raw,
            Target::Layer(index) => &mut raw[LAYERS][*index],
            Target::Inner { index, inner } => {
                let layer = &mut raw[LAYERS][*index];
                let overrides = &mut layer[OVERRIDES];
                if !overrides.is_object() {
                    *overrides = Value::Object(Map::new());
                }
                let entry = &mut overrides[Address::layer(inner.iter().cloned()).to_string()];
                if !entry.is_object() {
                    *entry = Value::Object(Map::new());
                }
                entry
            }
        }
    }

    /// Where validation reports `key` of this target, whose address is
    /// `address`.
    fn place_of(&self, address: &Address, key: &KeyPath) -> Place {
        match self {
            Target::Scene => Place::from(key.as_str()),
            Target::Layer(index) => {
                Place::layer(&address.to_string(), *index).join(Place::from(key.as_str()))
            }
            Target::Inner { .. } => Place::Scene(Address {
                layers: address.layers.clone(),
                keys: key.steps().to_vec(),
            }),
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
    crate::lock::check(doc, edit)?;
    let target = Target::find(doc, &edit.target)?;
    let mut raw = doc.raw().clone();
    let changes = slot::split(doc, edit, &mut raw)?;
    // An override replaces whole keys, so a change inside one starts from
    // the key as the instance draws it.
    let drawn = match &target {
        Target::Inner { .. } => doc.layer_json(&edit.target.layers),
        _ => None,
    };
    let object = target.object(&mut raw);
    for change in &changes {
        if let (Some(drawn), Some(object)) = (&drawn, object.as_object_mut()) {
            let first = change.key.first();
            let inside = change.key.steps().len() > 1;
            if inside
                && !object.contains_key(first)
                && let Some(value) = drawn.get(first)
            {
                object.insert(first.to_owned(), value.clone());
            }
        }
        match &change.value {
            EditValue::Set(value) => path::set(object, &change.key, value.clone())?,
            // Removing a missing key already gives what was asked for.
            EditValue::Remove => _ = path::remove(object, &change.key),
        }
    }
    if let Target::Inner { index, .. } = &target {
        tidy_overrides(&mut raw[LAYERS][*index]);
        removed_through_override(&raw, edit)?;
    }
    Ok((raw, target))
}

/// Drops an override an edit left empty, then `overrides` if it is empty.
fn tidy_overrides(layer: &mut Value) {
    let Some(layer) = layer.as_object_mut() else {
        return;
    };
    if let Some(overrides) = layer.get_mut(OVERRIDES).and_then(Value::as_object_mut) {
        overrides.retain(|_, keys| !keys.as_object().is_some_and(Map::is_empty));
    }
    if layer
        .get(OVERRIDES)
        .and_then(Value::as_object)
        .is_some_and(Map::is_empty)
    {
        layer.shift_remove(OVERRIDES);
    }
}

/// Rejects removing a key from an inner layer that its composition still
/// gives it: an override can change a key, not take it away.
fn removed_through_override(raw: &Value, edit: &Edit) -> Result<()> {
    let ids = &edit.target.layers;
    let Some(drawn) = motion_format::overrides::layer_json(raw, ids) else {
        return Ok(());
    };
    let kept = edit.changes.iter().find(|change| {
        change.value == EditValue::Remove
            && change.key.steps().len() == 1
            && drawn.contains_key(change.key.first())
    });
    let Some(change) = kept else {
        return Ok(());
    };
    let key = &change.key;
    let comp = comp_holding(raw, ids).unwrap_or_default();
    let id = ids.last().map(ToString::to_string).unwrap_or_default();
    Err(Diagnostic::new(
        Kind::Conflict,
        format!(
            "`{key}` of `{}` comes from composition `{comp}`; an override can change it, not remove it",
            edit.target
        ),
    )
    .expected(format!(
        "`{} {key}=<value>`, or `scene comps.{comp}.layers[{id}].{key}=none` for every layer that draws `{comp}`",
        edit.target
    )))
}

/// The id of the composition that holds the layer `ids` lead to.
fn comp_holding(raw: &Value, ids: &[Id]) -> Option<String> {
    let (_, outer) = ids.split_last()?;
    let parent = motion_format::overrides::layer_json(raw, outer)?;
    parent.get(COMP)?.as_str().map(str::to_owned)
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
                if error.path.as_ref() == Some(&target.place_of(&edit.target, &c.key)) =>
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
    let (allowed, what): (Vec<&str>, &str) = if edit.target.is_scene() {
        (SCENE_KEYS.to_vec(), "scene key")
    } else {
        (layer_keys().collect(), "layer key")
    };
    match edit
        .changes
        .iter()
        .filter(|change| !slot::is_slot(change))
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
