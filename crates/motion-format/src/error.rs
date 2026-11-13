//! The error types of the format: [`ShorthandError`] for one shorthand
//! string and [`FormatError`] for a whole document.
//!
//! Part of step 1 (Read). Every error says which forms would have been
//! accepted, so an agent can fix its input in one try.

use std::borrow::Cow;

use thiserror::Error;

/// The message of a [`FormatError`] for a key that holds something other than
/// a string. `Document` consumers can test for it with
/// [`FormatError::is_not_a_string`].
pub(crate) const MUST_BE_STRING: &str = "must be a string";

/// A shorthand string (such as a time, colour or size) could not be parsed.
///
/// The error always says which forms would have been accepted, so an agent
/// can correct its input without reading the documentation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}; expected {expected}")]
pub struct ShorthandError {
    /// What was wrong, including the offending input.
    pub message: String,
    /// The accepted forms, written for people, e.g. ``"`12f` or `0.4s`"``.
    pub expected: Cow<'static, str>,
}

impl ShorthandError {
    pub(crate) fn new(message: impl Into<String>, expected: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
            expected: expected.into(),
        }
    }
}

/// A scene document is invalid.
///
/// Carries the path of the offending key (`layers[1].in`, or `scene` for the
/// top level) so an agent knows exactly what to fix.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{path}: {message}{}", self.expected_suffix())]
pub struct FormatError {
    /// Where the problem is, e.g. `layers[1].in`.
    pub path: String,
    /// What was wrong.
    pub message: String,
    /// The accepted forms, when there is a short way to say them.
    pub expected: Option<String>,
}

impl FormatError {
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            expected: None,
        }
    }

    pub(crate) fn expecting(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    /// The `; expected …` tail of the message, or nothing when there is no
    /// short way to say the accepted forms.
    fn expected_suffix(&self) -> String {
        self.expected
            .as_ref()
            .map(|e| format!("; expected {e}"))
            .unwrap_or_default()
    }

    /// True when the key was given a number, boolean or other non-string
    /// value where a string is required.
    pub fn is_not_a_string(&self) -> bool {
        self.message == MUST_BE_STRING
    }

    /// Lifts a shorthand failure to a document error at `path`, keeping its
    /// message and accepted forms.
    pub(crate) fn from_shorthand(path: impl Into<String>, e: ShorthandError) -> Self {
        Self::new(path, e.message).expecting(e.expected)
    }
}

#[cfg(test)]
#[path = "../tests/unit/error.rs"]
mod tests;
