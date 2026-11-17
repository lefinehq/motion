//! Names the format has replaced and still reads ([D73]): the scene's
//! version key was `motion` before the project was named nakkash. An old
//! name is read as its new one, never written, and reported as a
//! [`Deprecation`] for the caller to show.
//!
//! [D73]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d73-the-project-is-named-nakkash

use std::fmt;

use serde_json::Value;

use crate::parse::key;
use crate::{Diagnostic, Kind, Result};

/// The `tracing` target of every deprecation note, which the CLI shows
/// whatever its log level.
pub const TARGET: &str = "nakkash::deprecated";

/// The scene keys that have a new name, as `(old, new)`.
pub const RENAMED_SCENE_KEYS: [(&str, &str); 1] = [(key::MOTION, key::NAKKASH)];

/// An old name a scene still uses: it reads as `new`, and the author should
/// write `new` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deprecation {
    /// The scene key as written.
    pub old: &'static str,
    /// The key that replaces it.
    pub new: &'static str,
}

impl fmt::Display for Deprecation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is the old name of the scene key `{}`, from before the project was renamed; write `{}` instead (D73)",
            self.old, self.new, self.new
        )
    }
}

/// Whether `name` is the old name of a scene key.
pub(crate) fn is_old_name(name: &str) -> bool {
    RENAMED_SCENE_KEYS.iter().any(|(old, _)| *old == name)
}

/// The old names the top level of `scene` uses.
pub(crate) fn in_scene(scene: &Value) -> Vec<Deprecation> {
    RENAMED_SCENE_KEYS
        .iter()
        .filter(|(old, _)| scene.get(old).is_some())
        .map(|&(old, new)| Deprecation { old, new })
        .collect()
}

/// A [`Kind::Conflict`] diagnostic when `scene` writes a key under both its
/// old and its new name.
pub(crate) fn no_name_twice(scene: &Value) -> Result<()> {
    for (old, new) in RENAMED_SCENE_KEYS {
        if scene.get(old).is_some() && scene.get(new).is_some() {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!("`{old}` is the old name of `{new}`, which the scene also has"),
            )
            .at(old)
            .expected(format!("only `{new}`")));
        }
    }
    Ok(())
}
