//! `motion-diagnostic`: the one error type of motion.
//!
//! This crate is not a pipeline step. Every step and side branch (format,
//! scene, eval, query, edit) and the CLI report failures as a [`Diagnostic`],
//! so a message reads the same wherever it comes from: what was wrong, where
//! it was (`layers[1].fill`), and which forms would have been accepted, so an
//! agent can fix its input in one try.
//!
//! ```
//! use motion_diagnostic::{Diagnostic, Kind};
//!
//! let d = Diagnostic::syntax("`12` is not a valid time")
//!     .expected("`<number>f` or `<number>s`")
//!     .at("in")
//!     .under("layers[1]");
//! assert_eq!(d.kind, Kind::Syntax);
//! assert_eq!(
//!     d.to_string(),
//!     "layers[1].in: `12` is not a valid time; expected `<number>f` or `<number>s`"
//! );
//! ```

use std::borrow::Cow;
use std::fmt;

/// What sort of problem a [`Diagnostic`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A shorthand string or an edit line that cannot be parsed.
    Syntax,
    /// A name that is not in its table: a key, preset, curve, property or
    /// layer id.
    UnknownName,
    /// A number outside the values its key accepts.
    OutOfRange,
    /// A required key or value that is absent.
    Missing,
    /// Two parts of the input that cannot both hold, such as a duplicate id.
    Conflict,
    /// A value that breaks a semantic rule of the format, such as a wrong
    /// JSON type or an id that does not follow the id rule.
    Invalid,
    /// A file that cannot be read or written.
    Io,
}

impl Kind {
    /// The exit code of the `motion` CLI for this kind of failure.
    ///
    /// Every kind exits 2 today. The code lives in this one method, and the
    /// match lists every kind, so a new kind has to choose its code here.
    pub const fn exit_code(self) -> u8 {
        match self {
            Kind::Syntax
            | Kind::UnknownName
            | Kind::OutOfRange
            | Kind::Missing
            | Kind::Conflict
            | Kind::Invalid
            | Kind::Io => 2,
        }
    }
}

/// A problem with the input, ready to show to people and agents.
///
/// Displays as `{path}: {message}; expected {expected}`, where the path and
/// the expected part are each left out when absent. Built with
/// [`Diagnostic::new`] (or a shorthand such as [`Diagnostic::syntax`]) and
/// the builder methods [`at`](Diagnostic::at), [`under`](Diagnostic::under)
/// and [`expected`](Diagnostic::expected).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct Diagnostic {
    /// What sort of problem this is; decides the CLI exit code.
    pub kind: Kind,
    /// Where the problem is, e.g. `layers[1].in`; `None` when the input has
    /// no structure to point into (an edit line, a time argument).
    pub path: Option<String>,
    /// What was wrong, including the offending input.
    pub message: String,
    /// The accepted forms, written for people, e.g. ``"`12f` or `0.4s`"``.
    pub expected: Option<Cow<'static, str>>,
}

/// The result of every fallible operation in motion.
pub type Result<T> = std::result::Result<T, Diagnostic>;

impl Diagnostic {
    /// A diagnostic of `kind` with `message`, without a path or expected form.
    pub fn new(kind: Kind, message: impl Into<String>) -> Self {
        Self {
            kind,
            path: None,
            message: message.into(),
            expected: None,
        }
    }

    /// A [`Kind::Syntax`] diagnostic: input that cannot be parsed.
    pub fn syntax(message: impl Into<String>) -> Self {
        Self::new(Kind::Syntax, message)
    }

    /// A [`Kind::UnknownName`] diagnostic for `name`, which is not one of
    /// `allowed`: ``unknown {what} `{name}`; expected one of a, b, c``.
    ///
    /// `allowed` should come from the table that holds the names, so the
    /// message can never drift from what is accepted.
    pub fn unknown(
        what: impl fmt::Display,
        name: impl fmt::Display,
        allowed: impl IntoIterator<Item = impl fmt::Display>,
    ) -> Self {
        let allowed: Vec<String> = allowed.into_iter().map(|a| a.to_string()).collect();
        Self::new(Kind::UnknownName, format!("unknown {what} `{name}`"))
            .expected(format!("one of {}", allowed.join(", ")))
    }

    /// Sets the path of the offending input, replacing any path already set.
    #[must_use]
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Places the diagnostic inside `prefix`: the path becomes
    /// `prefix.path`, or `prefix[i]…` when the path starts with an index
    /// segment, or just `prefix` when there was no path yet.
    ///
    /// An empty `prefix` (the top level) leaves the path as it is, so callers
    /// can nest without special-casing the root.
    #[must_use]
    pub fn under(mut self, prefix: impl Into<String>) -> Self {
        let mut full = prefix.into();
        if full.is_empty() {
            return self;
        }
        if let Some(path) = self.path.take() {
            // An index segment attaches directly: `layers` + `[1]` is `layers[1]`.
            if !path.starts_with('[') {
                full.push('.');
            }
            full.push_str(&path);
        }
        self.path = Some(full);
        self
    }

    /// Sets the accepted forms, replacing any already set.
    #[must_use]
    pub fn expected(mut self, form: impl Into<Cow<'static, str>>) -> Self {
        self.expected = Some(form.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{path}: ")?;
        }
        f.write_str(&self.message)?;
        if let Some(expected) = &self.expected {
            write!(f, "; expected {expected}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
