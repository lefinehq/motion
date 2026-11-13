//! The error type shared by parsing and applying edits.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). Messages are written for people and agents: they name what was
//! received and what was expected.

use std::fmt;

/// Why an edit line could not be parsed or applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError {
    /// The full human-readable message.
    pub message: String,
}

impl EditError {
    /// Builds an error from any string-like message.
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EditError {
    /// Prints the bare message, with no prefix.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EditError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_the_bare_message() {
        let e = EditError::new("no layer with id `ghost`");
        assert_eq!(e.to_string(), "no layer with id `ghost`");
        let boxed: Box<dyn std::error::Error> = Box::new(e);
        assert_eq!(boxed.to_string(), "no layer with id `ghost`");
    }
}
