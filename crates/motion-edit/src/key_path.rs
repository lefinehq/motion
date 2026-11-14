//! The key of a change: a dotted path such as `anim.opacity`.
//!
//! Part of the edit side branch (not a numbered rendering step). The edit
//! grammar in `crate::parse` reads a [`KeyPath`]; `crate::path` sets and
//! removes it in the raw document.

use std::fmt;
use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use motion_grammar::parse_all;

use crate::parse::path;
use crate::problem::Problem;

/// The key of a change: a dotted path such as `anim.opacity`, kept both as
/// written and as its segments, so neither has to be rebuilt from the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPath {
    /// The key as written, segments joined by `.`.
    text: String,
    /// The names of the path; never empty.
    segments: Vec<String>,
}

impl KeyPath {
    /// The key as written: `anim.opacity`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The names of the path, outermost first: `anim`, `opacity`. There is
    /// always at least one.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// The first name of the path: `anim` of `anim.opacity`.
    #[must_use]
    pub fn first(&self) -> &str {
        // A path always has a first segment, even when the key has no dot.
        &self.segments[0]
    }

    /// The key as written, up to (not including) segment `depth`: depth 2
    /// of `a.b.c` is `a.b`, and depth 0 is empty.
    pub(crate) fn prefix(&self, depth: usize) -> &str {
        let names: usize = self.segments[..depth].iter().map(String::len).sum();
        let dots = depth.saturating_sub(1);
        &self.text[..names + dots]
    }

    /// True when one key is the other or a prefix of it, segment by
    /// segment: `anim` and `anim.x`, but not `anim` and `animal`.
    pub(crate) fn overlaps(&self, other: &KeyPath) -> bool {
        self.segments.starts_with(&other.segments) || other.segments.starts_with(&self.segments)
    }
}

impl fmt::Display for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl KeyPath {
    /// A path of `segments`, written `text`; the grammar checks that the
    /// two agree.
    pub(crate) fn new(text: &str, segments: Vec<String>) -> KeyPath {
        KeyPath {
            text: text.to_owned(),
            segments,
        }
    }
}

impl FromStr for KeyPath {
    type Err = Diagnostic;

    /// Parses a key on its own, as the edit grammar reads it before `=`.
    fn from_str(text: &str) -> Result<KeyPath> {
        parse_all(text, path, |problem| {
            // The path grammar names every failure it has.
            problem.unwrap_or(Problem::InvalidKey(text)).diagnostic()
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/key_path.rs"]
mod tests;
