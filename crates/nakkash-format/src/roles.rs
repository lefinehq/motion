//! Roles ([D50]): what a master's layers do. A scene's `roles` map a role
//! name to a layer address, `"headline":"title"`, or to a slot,
//! `"accent":"$accent"`, so an edit and a brief can name the headline
//! without knowing where it is.
//!
//! [D50]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d50-roles-name-what-a-masters-layers-do

use std::fmt;

use serde_json::Value as Json;

use crate::parse::key::ROLES;
use crate::{Address, Diagnostic, Id, Kind, Place, Result, Scene, Step};

/// What a role names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleTarget {
    /// A layer at any depth.
    Layer(Address),
    /// A slot of the scene.
    Slot(Id),
}

impl fmt::Display for RoleTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoleTarget::Layer(address) => write!(f, "{address}"),
            RoleTarget::Slot(id) => write!(f, "${id}"),
        }
    }
}

/// One role of a scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    /// Its name, which an edit writes after `@`.
    pub id: Id,
    /// What it names.
    pub target: RoleTarget,
}

const FORM: &str = r#"an object of layer addresses or slots by role, e.g. {"headline":"title","cta":"end/cta","accent":"$accent"}"#;

/// The roles `written` declares in `scene`, each naming a layer it draws
/// or a slot it declares.
///
/// # Errors
/// An invalid diagnostic for roles that are not an object of strings or
/// an id that is not one, and an unknown-name one for a role naming no
/// layer or slot, each at `roles.<name>`.
pub(crate) fn read(written: Option<&Json>, scene: &Scene) -> Result<Vec<Role>> {
    let Some(written) = written else {
        return Ok(Vec::new());
    };
    let Json::Object(roles) = written else {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("`{written}` is not roles"))
                .expected(FORM)
                .at(ROLES),
        );
    };
    roles
        .iter()
        .map(|(name, target)| {
            let at = Place::Scene(Address {
                layers: Vec::new(),
                keys: vec![Step::Key(ROLES.to_owned()), Step::Key(name.clone())],
            });
            role(name, target, scene).map_err(|d| d.at(at))
        })
        .collect()
}

fn role(name: &str, target: &Json, scene: &Scene) -> Result<Role> {
    let id: Id = name.parse()?;
    let Json::String(text) = target else {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("`{target}` is not a layer or a slot"),
        )
        .expected(FORM));
    };
    let target = match text.strip_prefix('$') {
        Some(slot) => {
            let slot: Id = slot.parse()?;
            if !scene.root.slots.iter().any(|s| s.id == slot) {
                let known = scene.root.slots.iter().map(|s| format!("${}", s.id));
                return Err(Diagnostic::unknown("slot", text, known));
            }
            RoleTarget::Slot(slot)
        }
        None => {
            let address: Address = text.parse()?;
            if !address.keys.is_empty() || scene.layer(&address.layers).is_none() {
                return Err(Diagnostic::unknown("layer", text, scene.addresses()));
            }
            RoleTarget::Layer(address)
        }
    };
    Ok(Role { id, target })
}

impl Scene {
    /// The role called `id`, if the scene has one.
    pub fn role(&self, id: &str) -> Option<&Role> {
        self.roles.iter().find(|role| &*role.id == id)
    }
}

#[cfg(test)]
#[path = "../tests/unit/roles.rs"]
mod tests;
