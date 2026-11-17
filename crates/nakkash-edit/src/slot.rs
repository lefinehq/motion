//! Edits of a slot ([D44]): `$accent=#2A9D8F` on the scene sets the value
//! its slot declares, and `card-2 $title=Sale` on a `comp` layer gives the
//! composition's slot a value for that instance alone, under `overrides`.
//!
//! [D44]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d44-slots-are-named-values-that-layers-follow

use nakkash_format::key::{OVERRIDES, SLOTS};
use nakkash_format::{Content, Diagnostic, Document, Kind, Result, SlotType, Step};
use serde_json::{Map, Value};

use crate::{Change, Edit, EditValue, KeyPath};

/// Whether `change` edits a slot, its key written `$id`.
pub(crate) fn is_slot(change: &Change) -> bool {
    change.key.first().starts_with('$')
}

/// The slot changes of `edit` made on the scene's slots in `raw`, the
/// others left. On a layer, each slot change becomes the key
/// `overrides.$id`, which the layer must have a composition for.
///
/// # Errors
/// An unknown-name diagnostic for a slot the scene does not declare, a
/// conflict for a slot removed while values follow it, and an invalid one
/// for a slot given to a layer that draws no composition.
pub(crate) fn split(doc: &Document, edit: &Edit, raw: &mut Value) -> Result<Vec<Change>> {
    let mut rest = Vec::with_capacity(edit.changes.len());
    for change in &edit.changes {
        if !is_slot(change) {
            rest.push(change.clone());
        } else if edit.target.is_scene() {
            set_scene_slot(doc, raw, &change.key.first()[1..], &change.value)?;
        } else {
            draws_a_comp(doc, edit, &change.key)?;
            let slot = change.key.first();
            rest.push(Change {
                key: KeyPath::new(
                    &format!("{OVERRIDES}.{slot}"),
                    vec![Step::Key(OVERRIDES.to_owned()), Step::Key(slot.to_owned())],
                ),
                value: change.value.clone(),
            });
        }
    }
    Ok(rest)
}

fn draws_a_comp(doc: &Document, edit: &Edit, key: &KeyPath) -> Result<()> {
    match doc.scene().layer(&edit.target.layers).map(|l| &l.content) {
        Some(Content::Comp(_)) => Ok(()),
        _ => Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "layer `{}` draws no composition, so it has no slot `{key}`",
                edit.target
            ),
        )
        .expected(format!(
            "a slot of the scene, `{key}=<value>`, or of a `comp` layer, `<layer> {key}=<value>`"
        ))),
    }
}

/// Sets or removes the scene's slot `id`, keeping the form it is written in.
fn set_scene_slot(doc: &Document, raw: &mut Value, id: &str, value: &EditValue) -> Result<()> {
    let listed = doc.slots();
    let own = || listed.iter().filter(|l| l.comp.is_none());
    let Some(slot) = own().find(|l| &*l.slot.id == id) else {
        let unknown = Diagnostic::unknown(
            "slot",
            format!("${id}"),
            own().map(|l| format!("${}", l.slot.id)),
        );
        return Err(match own().next() {
            Some(_) => unknown,
            None => unknown.expected(format!(
                "a slot the scene declares; declare it with `scene {SLOTS}.{id}=<value>`"
            )),
        });
    };
    let slots = raw[SLOTS]
        .as_object_mut()
        .expect("a scene that declares a slot has `slots`");
    match value {
        EditValue::Remove if !slot.followers.is_empty() => Err(Diagnostic::new(
            Kind::Conflict,
            format!(
                "slot `${id}` cannot go while values follow it: {}",
                slot.followers.join(", ")
            ),
        )
        .expected(format!(
            "a new value, `${id}=<value>`, or those values written without it first"
        ))),
        EditValue::Remove => {
            slots.shift_remove(id);
            if slots.is_empty() {
                raw.as_object_mut().map(|scene| scene.shift_remove(SLOTS));
            }
            Ok(())
        }
        EditValue::Set(new) => {
            let ty = slot.slot.ty;
            let written = slots.get_mut(id).expect("a listed slot is written");
            match written {
                Value::Object(fields) => _ = fields.insert(ty.name().to_owned(), new.clone()),
                // A value of another type is written out, so that the type
                // stays the slot's and the check names it.
                _ if shorthand(new) == Some(ty) => *written = new.clone(),
                _ => {
                    *written = Value::Object(Map::from_iter([(ty.name().to_owned(), new.clone())]))
                }
            }
            Ok(())
        }
    }
}

/// The type a slot written as `value` alone takes.
fn shorthand(value: &Value) -> Option<SlotType> {
    match value {
        Value::Number(_) => Some(SlotType::Number),
        Value::Array(xy) if xy.len() == 2 => Some(SlotType::Point),
        Value::Array(_) => Some(SlotType::Color),
        Value::String(_) => Some(SlotType::Color),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/slot.rs"]
mod tests;
