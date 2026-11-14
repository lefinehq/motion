//! [`Diagnostic`], the one error type of every motion crate: what was wrong,
//! where (`layers[1].fill`) and which forms are accepted, so an agent can fix
//! its input in one try.
//!
//! ```
//! use motion_format::{Diagnostic, Kind};
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
use std::path::Path;
use std::{fmt, io};

/// What sort of problem a [`Diagnostic`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A shorthand string or an edit line that cannot be parsed.
    Syntax,
    /// A name that is not in its table: a key, preset, curve, property or id.
    UnknownName,
    /// A number outside the values its key accepts.
    OutOfRange,
    /// A required key or value that is absent.
    Missing,
    /// Two parts of the input that cannot both hold, such as a duplicate id.
    Conflict,
    /// A JSON value of the wrong type, such as a number for a string.
    WrongType,
    /// A rule the value breaks, such as the id rule.
    Invalid,
    /// A file that cannot be read or written.
    Io,
}

impl Kind {
    /// The exit code of the `motion` CLI for this kind of failure.
    ///
    /// The match names every kind, so a new kind has to choose its code here.
    pub const fn exit_code(self) -> u8 {
        match self {
            Kind::Syntax
            | Kind::UnknownName
            | Kind::OutOfRange
            | Kind::Missing
            | Kind::Conflict
            | Kind::WrongType
            | Kind::Invalid
            | Kind::Io => 2,
        }
    }
}

/// A problem with the input, shown as `{path}: {message}; expected {expected}`,
/// without the parts that are absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// What sort of problem this is; decides the CLI exit code.
    pub kind: Kind,
    /// Where the problem is, e.g. `layers[1].in`; `None` for input without structure.
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

    /// A [`Kind::UnknownName`] diagnostic: ``unknown {what} `{name}`; expected one of a, b``.
    ///
    /// Pass `allowed` from the table that holds the names, so the list cannot drift.
    pub fn unknown(
        what: impl fmt::Display,
        name: impl fmt::Display,
        allowed: impl IntoIterator<Item = impl fmt::Display>,
    ) -> Self {
        let allowed: Vec<String> = allowed.into_iter().map(|a| a.to_string()).collect();
        Self::new(Kind::UnknownName, format!("unknown {what} `{name}`"))
            .expected(format!("one of {}", allowed.join(", ")))
    }

    /// A [`Kind::Io`] diagnostic: ``cannot {action} `{path}`: {err}``.
    ///
    /// The file goes in the message, since [`path`](Diagnostic::path) points into a document.
    pub fn io(action: &str, path: &Path, err: &io::Error) -> Self {
        Self::new(
            Kind::Io,
            format!("cannot {action} `{}`: {err}", path.display()),
        )
    }

    /// Sets the path of the offending input, replacing any path already set.
    #[must_use]
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Places the diagnostic inside `prefix`: `prefix.path`, `prefix[i]…` or `prefix`.
    ///
    /// An empty `prefix` (the top level) keeps the path, so callers need no root case.
    #[must_use]
    pub fn under(mut self, prefix: impl Into<String>) -> Self {
        let mut full = prefix.into();
        if full.is_empty() {
            return self;
        }
        if let Some(path) = self.path.take() {
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

    /// Adds an accepted form after those already set, as `{expected}, or {form}`.
    #[must_use]
    pub fn or_expected(mut self, form: impl fmt::Display) -> Self {
        let joined = match self.expected.take() {
            Some(expected) => format!("{expected}, or {form}"),
            None => form.to_string(),
        };
        self.expected = Some(joined.into());
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

impl std::error::Error for Diagnostic {}

#[cfg(test)]
#[path = "../tests/unit/diagnostic.rs"]
mod tests;
