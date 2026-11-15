//! Setting and removing a key path, such as `anim.opacity` or
//! `fx[glow].radius`, in raw JSON, so that keys keep the order the author
//! wrote them in. A selector picks a list element: `[glow]` the one whose
//! `id` is `glow`, `[0]` the first.

use motion_format::address::key_text;
use motion_format::key::ID;
use motion_format::{Diagnostic, Kind, Result, Step};
use serde_json::{Map, Value};

use crate::KeyPath;

/// Sets `key` in `value` to `new`, creating the objects on the way. An
/// existing key keeps its position, and a new one is appended; a selector
/// only ever picks an element that is there.
///
/// # Errors
/// A [`Kind::WrongType`] diagnostic when a key meets something other than
/// an object, or a selector something other than a list, and a
/// [`Kind::UnknownName`] one for a selector that picks no element.
pub fn set(value: &mut Value, key: &KeyPath, new: Value) -> Result<()> {
    let mut slot = value;
    for (depth, step) in key.steps().iter().enumerate() {
        slot = match step {
            Step::Key(name) => {
                let Value::Object(object) = slot else {
                    return Err(not_a(key, depth, "an object"));
                };
                object
                    .entry(name.as_str())
                    .or_insert_with(|| Value::Object(Map::new()))
            }
            selector => element(slot, selector).map_err(|e| e.of(key, depth))?,
        };
    }
    *slot = new;
    Ok(())
}

/// Removes `key` from `value`, then every object this left empty, but never
/// `value` itself; a selector last removes the element from its list.
/// Returns whether anything was removed.
pub fn remove(value: &mut Value, key: &KeyPath) -> bool {
    remove_steps(value, key.steps())
}

/// [`remove`] for the steps of a path, outermost first.
fn remove_steps(value: &mut Value, path: &[Step]) -> bool {
    let Some((first, rest)) = path.split_first() else {
        return false;
    };
    match (value, first) {
        (Value::Object(object), Step::Key(name)) if rest.is_empty() => {
            object.shift_remove(name).is_some()
        }
        (Value::Object(object), Step::Key(name)) => {
            let Some(child) = object.get_mut(name) else {
                return false;
            };
            let removed = remove_steps(child, rest);
            // An emptied `anim: {}` says nothing; drop it too.
            if removed && child.as_object().is_some_and(Map::is_empty) {
                object.shift_remove(name);
            }
            removed
        }
        (Value::Array(list), selector) => match position(list, selector) {
            Some(i) if rest.is_empty() => {
                list.remove(i);
                true
            }
            // An element emptied inside is still an element of its list.
            Some(i) => remove_steps(&mut list[i], rest),
            None => false,
        },
        _ => false,
    }
}

/// Where in `list` the selector `step` points: the element whose `id` it
/// names, or the one at its index.
fn position(list: &[Value], step: &Step) -> Option<usize> {
    match step {
        Step::Id(id) => list
            .iter()
            .position(|item| item.get(ID).and_then(Value::as_str) == Some(id.as_ref())),
        Step::Index(index) => (*index < list.len()).then_some(*index),
        Step::Key(_) => None,
    }
}

/// Why a step of a key path could not be followed: it is filled in with the
/// path when the caller knows it.
struct Missed {
    kind: Kind,
    /// What the place reached should have been, or what was not in it.
    what: String,
}

impl Missed {
    /// The diagnostic for following `key` up to step `depth`.
    fn of(self, key: &KeyPath, depth: usize) -> Diagnostic {
        match self.kind {
            Kind::WrongType => not_a(key, depth, &self.what),
            kind => Diagnostic::new(
                kind,
                format!(
                    "cannot set `{key}`: `{}` has no element {}",
                    key.prefix(depth),
                    self.what
                ),
            ),
        }
    }
}

/// The element of the list `slot` the selector `step` picks.
fn element<'v>(slot: &'v mut Value, step: &Step) -> std::result::Result<&'v mut Value, Missed> {
    let Value::Array(list) = slot else {
        return Err(Missed {
            kind: Kind::WrongType,
            what: "a list".into(),
        });
    };
    let what = key_text(std::slice::from_ref(step));
    match position(list, step) {
        Some(i) => Ok(&mut list[i]),
        None => Err(Missed {
            kind: Kind::UnknownName,
            what: format!("`{what}`"),
        }),
    }
}

/// `key` cannot be followed past step `depth`: what is there is not `what`.
fn not_a(key: &KeyPath, depth: usize, what: &str) -> Diagnostic {
    let parent = match key.prefix(depth).as_str() {
        "" => "the edited value".to_owned(),
        parent => format!("`{parent}`"),
    };
    Diagnostic::new(
        Kind::WrongType,
        format!("cannot set `{key}`: {parent} is not {what}"),
    )
}

#[cfg(test)]
#[path = "../tests/unit/path.rs"]
mod tests;
