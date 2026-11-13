//! The error types of the format: [`ShorthandError`] for one shorthand
//! string and [`FormatError`] for a whole document.
//!
//! Part of step 1 (Read). Every error says which forms would have been
//! accepted, so an agent can fix its input in one try.

use std::fmt;

/// The message of a [`FormatError`] for a key that holds something other than
/// a string. `Document` consumers can test for it with
/// [`FormatError::is_not_a_string`].
pub(crate) const MUST_BE_STRING: &str = "must be a string";

/// A shorthand string (such as a time, colour or size) could not be parsed.
///
/// The error always says which forms would have been accepted, so an agent
/// can correct its input without reading the documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShorthandError {
    /// What was wrong, including the offending input.
    pub message: String,
    /// The accepted forms, written for people, e.g. ``"`12f` or `0.4s`"``.
    pub expected: &'static str,
}

impl ShorthandError {
    pub(crate) fn new(message: impl Into<String>, expected: &'static str) -> Self {
        Self {
            message: message.into(),
            expected,
        }
    }
}

impl fmt::Display for ShorthandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}; expected {}", self.message, self.expected)
    }
}

impl std::error::Error for ShorthandError {}

/// A scene document is invalid.
///
/// Carries the path of the offending key (`layers[1].in`, or `scene` for the
/// top level) so an agent knows exactly what to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)?;
        if let Some(e) = &self.expected {
            write!(f, "; expected {e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for FormatError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_joins_message_and_expected_form() {
        let e = ShorthandError::new("bad value `x`", "`<number>f`");
        assert_eq!(e.to_string(), "bad value `x`; expected `<number>f`");
    }

    #[test]
    fn format_error_display_with_and_without_expected() {
        let e = FormatError::new("layers[1].in", "unknown preset `zoom`");
        assert_eq!(e.to_string(), "layers[1].in: unknown preset `zoom`");
        let e = e.expecting("`fade`");
        assert_eq!(
            e.to_string(),
            "layers[1].in: unknown preset `zoom`; expected `fade`"
        );
    }

    #[test]
    fn not_a_string_is_recognised() {
        assert!(FormatError::new("text", MUST_BE_STRING).is_not_a_string());
        assert!(!FormatError::new("text", "invalid").is_not_a_string());
    }

    #[test]
    fn shorthand_errors_convert_with_path() {
        let s = ShorthandError::new("bad value `x`", "`<number>f`");
        let e = FormatError::from_shorthand("dur", s);
        assert_eq!(e.to_string(), "dur: bad value `x`; expected `<number>f`");
    }
}
