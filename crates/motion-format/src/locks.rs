//! Locks ([D51]): what a variant may not change. A scene's `locks` list
//! roles (`@logo`), layers and everything inside them (`end`), slots
//! (`$accent`), and two named locks: `palette`, only the colours the scene
//! already has, and `typefaces`, only the font families it already uses.
//! Edits are checked against them where they are made.
//!
//! [D51]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d51-locks-are-refused-in-the-edit

use std::fmt;

use serde_json::Value as Json;

use crate::parse::key::LOCKS;
use crate::{Address, Diagnostic, Id, Kind, Place, Result, Scene, Step};

/// The lock on colours.
pub const PALETTE: &str = "palette";
/// The lock on font families.
pub const TYPEFACES: &str = "typefaces";

/// One lock of a scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lock {
    /// What a role names.
    Role(Id),
    /// A layer and everything inside it.
    Layer(Address),
    /// A slot of the scene.
    Slot(Id),
    /// Only the colours the scene already has.
    Palette,
    /// Only the font families the scene already uses.
    Typefaces,
}

impl fmt::Display for Lock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lock::Role(id) => write!(f, "@{id}"),
            Lock::Layer(address) => write!(f, "{address}"),
            Lock::Slot(id) => write!(f, "${id}"),
            Lock::Palette => f.write_str(PALETTE),
            Lock::Typefaces => f.write_str(TYPEFACES),
        }
    }
}

const FORM: &str = r#"a list of roles, layers, slots, `palette` and `typefaces`, e.g. ["@logo","end","$accent","palette"]"#;

/// The locks `written` lists in `scene`, each naming a role, a layer or a
/// slot it has, or a named lock.
///
/// # Errors
/// An invalid diagnostic for locks that are not a list of strings, and an
/// unknown-name one for a lock naming nothing, each at `locks[i]`.
pub(crate) fn read(written: Option<&Json>, scene: &Scene) -> Result<Vec<Lock>> {
    let Some(written) = written else {
        return Ok(Vec::new());
    };
    let Json::Array(locks) = written else {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("`{written}` is not locks"))
                .expected(FORM)
                .at(LOCKS),
        );
    };
    locks
        .iter()
        .enumerate()
        .map(|(i, lock)| {
            let at = Place::Scene(Address {
                layers: Vec::new(),
                keys: vec![Step::Key(LOCKS.to_owned()), Step::Index(i)],
            });
            one(lock, scene).map_err(|d| d.at(at))
        })
        .collect()
}

fn one(written: &Json, scene: &Scene) -> Result<Lock> {
    let Json::String(text) = written else {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("`{written}` is not a lock")).expected(FORM),
        );
    };
    Ok(match text.as_str() {
        PALETTE => Lock::Palette,
        TYPEFACES => Lock::Typefaces,
        _ if text.starts_with('@') => {
            let role: Id = text[1..].parse()?;
            if scene.role(&role).is_none() {
                let known = scene.roles.iter().map(|r| format!("@{}", r.id));
                return Err(Diagnostic::unknown("role", text, known));
            }
            Lock::Role(role)
        }
        _ if text.starts_with('$') => {
            let slot: Id = text[1..].parse()?;
            if !scene.root.slots.iter().any(|s| s.id == slot) {
                let known = scene.root.slots.iter().map(|s| format!("${}", s.id));
                return Err(Diagnostic::unknown("slot", text, known));
            }
            Lock::Slot(slot)
        }
        _ => {
            let address: Address = text.parse()?;
            if !address.keys.is_empty() || scene.layer(&address.layers).is_none() {
                return Err(Diagnostic::unknown("layer", text, scene.addresses()));
            }
            Lock::Layer(address)
        }
    })
}

impl Scene {
    /// The scene's locks, a role's as the layer or slot it names, each
    /// with the lock as written.
    pub fn held_locks(&self) -> Vec<(Lock, &Lock)> {
        self.locks
            .iter()
            .filter_map(|lock| match lock {
                Lock::Role(role) => {
                    let role = self.role(role)?;
                    let held = match &role.target {
                        crate::RoleTarget::Layer(address) => Lock::Layer(address.clone()),
                        crate::RoleTarget::Slot(slot) => Lock::Slot(slot.clone()),
                    };
                    Some((held, lock))
                }
                other => Some((other.clone(), lock)),
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "../tests/unit/locks.rs"]
mod tests;
