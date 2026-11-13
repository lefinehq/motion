//! The error type shared by every shorthand parser.

use std::fmt;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_joins_message_and_expected_form() {
        let e = ShorthandError::new("bad value `x`", "`<number>f`");
        assert_eq!(e.to_string(), "bad value `x`; expected `<number>f`");
    }
}
