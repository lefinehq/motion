//! The key of a change: a path such as `anim.opacity` or `fx[glow].radius`.

use std::str::FromStr;

use motion_format::address::key_text;
use motion_format::grammar::parse_all;
use motion_format::{Diagnostic, Result, Step};

use crate::parse::path;
use crate::problem::Problem;

/// The key of a change, such as `anim.opacity` or `fx[glow].radius`, kept
/// both as written and as its steps, so neither has to be rebuilt from the
/// other. It prints as written.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("{text}")]
pub struct KeyPath {
    /// The key as written.
    text: String,
    /// The steps of the path: keys, and `[id]` or `[index]` selectors
    /// after them; never empty, and always a key first.
    steps: Vec<Step>,
}

impl KeyPath {
    /// A path of `steps`, written `text`; the grammar checks that they agree.
    pub(crate) fn new(text: &str, steps: Vec<Step>) -> KeyPath {
        KeyPath {
            text: text.to_owned(),
            steps,
        }
    }

    /// The key as written: `anim.opacity`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The steps of the path, outermost first; there is always one, a key.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The first key of the path: `anim` of `anim.opacity`.
    #[must_use]
    pub fn first(&self) -> &str {
        match &self.steps[0] {
            Step::Key(key) => key,
            Step::Id(_) | Step::Index(_) => unreachable!("a key path starts with a key"),
        }
    }

    /// The path up to (not including) step `depth`: depth 2 of `a.b.c` is
    /// `a.b`, depth 2 of `fx[glow].radius` is `fx[glow]`, and depth 0 is empty.
    pub(crate) fn prefix(&self, depth: usize) -> String {
        key_text(&self.steps[..depth])
    }

    /// True when one key is the other or its prefix, step by step:
    /// `anim` and `anim.x`, but not `anim` and `animal`.
    pub(crate) fn overlaps(&self, other: &KeyPath) -> bool {
        self.steps.starts_with(&other.steps) || other.steps.starts_with(&self.steps)
    }
}

impl FromStr for KeyPath {
    type Err = Diagnostic;

    /// Parses a key on its own, as the edit grammar reads it before `=`.
    fn from_str(text: &str) -> Result<KeyPath> {
        // The path grammar names every failure it has.
        parse_all(text, path, |problem| {
            problem.unwrap_or(Problem::InvalidKey(text)).diagnostic()
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/key_path.rs"]
mod tests;
