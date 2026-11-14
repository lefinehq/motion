//! Ids from names: lower case ASCII letters and digits, `-` for every run of
//! other characters, starting with a letter, and a number added when two
//! names in one scope would share an id.

use std::collections::HashSet;

/// The ids given out in one scope: a scene's compositions, or one
/// composition's layers.
#[derive(Debug, Default)]
pub(super) struct Ids {
    /// The ids taken so far.
    taken: HashSet<String>,
}

impl Ids {
    /// A new id for `name`, or for `fallback` when the name has no letter or
    /// digit; unique in this scope.
    pub(super) fn give(&mut self, name: &str, fallback: &str) -> String {
        let base = match slug(name) {
            slug if slug.is_empty() => fallback.to_owned(),
            slug if slug.starts_with(|c: char| c.is_ascii_lowercase()) => slug,
            slug => format!("{fallback}-{slug}"),
        };
        let id = (1..)
            .map(|n| match n {
                1 => base.clone(),
                n => format!("{base}-{n}"),
            })
            .find(|id| !self.taken.contains(id))
            .expect("some number is free");
        self.taken.insert(id.clone());
        id
    }
}

/// `name` in lower case, with `-` for every run of characters that are not
/// ASCII letters or digits, and none at either end.
fn slug(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
#[path = "../../tests/unit/map/ids.rs"]
mod tests;
