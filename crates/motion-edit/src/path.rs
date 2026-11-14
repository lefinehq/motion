//! Setting and removing a dotted path, such as `anim.opacity`, in a JSON
//! value.
//!
//! Part of the edit side branch (not a numbered rendering step): `apply`
//! uses these on the raw document, so that keys keep the order the author
//! wrote them in.

use motion_diagnostic::{Diagnostic, Kind, Result};
use serde_json::{Map, Value};

use crate::KeyPath;

/// Sets `key` in `value` to `new`, creating the objects it needs on the
/// way. An existing key keeps its position (serde_json's `preserve_order`);
/// a new key is appended.
///
/// # Errors
/// A [`Kind::WrongType`] diagnostic when `value`, or a value on the way, is
/// not an object: ``cannot set `in.opacity`: `in` is not an object``. The
/// edit's target is always an object, but a root that is not one is named
/// as such: ``cannot set `in`: the edited value is not an object``.
pub fn set(value: &mut Value, key: &KeyPath, new: Value) -> Result<()> {
    let mut slot = value;
    for (depth, segment) in key.segments().iter().enumerate() {
        let Value::Object(object) = slot else {
            let parent = match key.prefix(depth) {
                "" => "the edited value".to_owned(),
                parent => format!("`{parent}`"),
            };
            return Err(Diagnostic::new(
                Kind::WrongType,
                format!("cannot set `{key}`: {parent} is not an object"),
            ));
        };
        slot = object
            .entry(segment.as_str())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    *slot = new;
    Ok(())
}

/// Removes the value at `key` from `value`, then every object this left
/// empty on the way back up, but never `value` itself. Returns whether
/// something was removed: a path that is not there, or that runs through a
/// value that is not an object, changes nothing.
pub fn remove(value: &mut Value, key: &KeyPath) -> bool {
    remove_segments(value, key.segments())
}

/// [`remove`] for the segments of a path, outermost first.
fn remove_segments(value: &mut Value, path: &[String]) -> bool {
    match (value, path) {
        (Value::Object(object), [leaf]) => object.shift_remove(leaf).is_some(),
        (Value::Object(object), [first, rest @ ..]) => {
            let Some(child) = object.get_mut(first) else {
                return false;
            };
            let removed = remove_segments(child, rest);
            // Removing `anim.opacity` from an `anim` with no other track
            // leaves `anim: {}`, which says nothing; drop it too.
            if removed && child.as_object().is_some_and(Map::is_empty) {
                object.shift_remove(first);
            }
            removed
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "../tests/unit/path.rs"]
mod tests;
