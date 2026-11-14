//! The key of a change: a dotted path such as `anim.opacity`.

use std::str::FromStr;

use motion_format::grammar::parse_all;
use motion_format::{Diagnostic, Result};

use crate::parse::path;
use crate::problem::Problem;

/// The key of a change, such as `anim.opacity`, kept both as written and as
/// its segments, so neither has to be rebuilt from the other. It prints as written.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("{text}")]
pub struct KeyPath {
    /// The key as written.
    text: String,
    /// The names of the path; never empty.
    segments: Vec<String>,
}

impl KeyPath {
    /// A path of `segments`, written `text`; the grammar checks that they agree.
    pub(crate) fn new(text: &str, segments: Vec<String>) -> KeyPath {
        KeyPath {
            text: text.to_owned(),
            segments,
        }
    }

    /// The key as written: `anim.opacity`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The names of the path, outermost first; there is always one.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// The first name of the path: `anim` of `anim.opacity`.
    #[must_use]
    pub fn first(&self) -> &str {
        &self.segments[0]
    }

    /// The key as written, up to (not including) segment `depth`: depth 2 of
    /// `a.b.c` is `a.b`, and depth 0 is empty.
    pub(crate) fn prefix(&self, depth: usize) -> &str {
        let names: usize = self.segments[..depth].iter().map(String::len).sum();
        let dots = depth.saturating_sub(1);
        &self.text[..names + dots]
    }

    /// True when one key is the other or its prefix, segment by segment:
    /// `anim` and `anim.x`, but not `anim` and `animal`.
    pub(crate) fn overlaps(&self, other: &KeyPath) -> bool {
        self.segments.starts_with(&other.segments) || other.segments.starts_with(&self.segments)
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
