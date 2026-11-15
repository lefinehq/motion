//! Roles the import proposes ([D50]): a layer named `@headline Title`
//! takes the role `headline`, and otherwise a role whose name only one
//! layer has, `logo` or `title`, takes that layer. Each proposal is in the
//! report, for the designer to confirm.
//!
//! [D50]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d50-roles-name-what-a-masters-layers-do

use motion_format::{Address, Content, Document};
use serde::Serialize;
use serde_json::{Map, Value as Json};

/// The words a layer's id is matched against, by role.
const VOCABULARY: [(&str, &[&str]); 9] = [
    ("headline", &["headline", "title", "heading"]),
    (
        "subhead",
        &["subtitle", "subhead", "subheadline", "tagline", "slogan"],
    ),
    ("body", &["body", "description", "paragraph", "copy"]),
    ("cta", &["cta", "button"]),
    ("logo", &["logo", "logotype", "brand"]),
    ("product", &["product", "packshot"]),
    ("background", &["bg", "background", "backdrop"]),
    ("price", &["price"]),
    ("date", &["date"]),
];

/// The locks a master with a logo gets ([D51]).
///
/// [D51]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d51-locks-are-refused-in-the-edit
pub(super) const DEFAULT_LOCKS: [&str; 3] = ["@logo", "palette", "typefaces"];

/// A role the import proposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposedRole {
    /// The role.
    pub role: String,
    /// The layer, by address.
    pub address: String,
    /// Why: the layer is named for it, or its id is the only one that
    /// matches.
    pub from: &'static str,
}

/// A layer named `@role Name`, as its role and the name its id comes from;
/// any other name as itself. The role must be one of the vocabulary's, so
/// a name such as `@YourChannel`, a handle shown as text, is a name.
pub(super) fn named(name: &str) -> (Option<String>, &str) {
    let Some(rest) = name.strip_prefix('@') else {
        return (None, name);
    };
    let (word, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let role = word.to_ascii_lowercase();
    match VOCABULARY.iter().any(|(known, _)| *known == role) {
        true if rest.trim().is_empty() => (Some(role), word),
        true => (Some(role), rest.trim()),
        false => (None, name),
    }
}

/// A layer the walk met named for a role: the role, the composition it is
/// in by id (none for the scene's own) and its id.
pub(super) type Named = (String, Option<String>, String);

/// The roles to propose for `doc`: those its layers are named for, then
/// each role of the vocabulary that one layer alone matches. A layer in a
/// composition is named through the `comp` layer that draws it, the first
/// when several do.
pub(super) fn propose(doc: &Document, named: &[Named]) -> Vec<ProposedRole> {
    let scene = doc.scene();
    let addresses = scene.addresses();
    // Where the layer `id` of composition `comp` is drawn.
    let drawn_at = |comp: &Option<String>, id: &str| -> Vec<&Address> {
        addresses
            .iter()
            .filter(|a| a.layers.last().is_some_and(|last| last.as_ref() == id))
            .filter(|a| match (comp, a.layers.split_last()) {
                (None, Some((_, outer))) => outer.is_empty(),
                (Some(comp), Some((_, outer))) => matches!(
                    scene.layer(outer).map(|l| &l.content),
                    Some(Content::Comp(drawn)) if &drawn.id == comp
                ),
                (_, None) => false,
            })
            .collect()
    };
    let mut proposed: Vec<ProposedRole> = Vec::new();
    for (role, comp, id) in named {
        if proposed.iter().any(|p| &p.role == role) {
            continue;
        }
        if let Some(address) = drawn_at(comp, id).first() {
            proposed.push(ProposedRole {
                role: role.clone(),
                address: address.to_string(),
                from: "named for it",
            });
        }
    }
    for (role, words) in VOCABULARY {
        if proposed.iter().any(|p| p.role == role) {
            continue;
        }
        let matching: Vec<&Address> = addresses
            .iter()
            .filter(|a| a.layers.last().is_some_and(|id| matches(id, words)))
            .collect();
        if let [address] = matching.as_slice() {
            proposed.push(ProposedRole {
                role: role.to_owned(),
                address: address.to_string(),
                from: "from its name",
            });
        }
    }
    proposed
}

/// Whether one of the `-` parts of `id`, its digits aside, is one of
/// `words`: `title-02` is a title, `subtitle` is not.
fn matches(id: &str, words: &[&str]) -> bool {
    id.split('-')
        .map(|part| part.trim_end_matches(|c: char| c.is_ascii_digit()))
        .any(|part| words.contains(&part))
}

/// The proposals as the scene writes them.
pub(super) fn written(proposed: &[ProposedRole]) -> Json {
    let roles: Map<String, Json> = proposed
        .iter()
        .map(|p| (p.role.clone(), Json::String(p.address.clone())))
        .collect();
    Json::Object(roles)
}

#[cfg(test)]
#[path = "../../tests/unit/map/roles.rs"]
mod tests;
