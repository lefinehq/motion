//! The error type shared by parsing and applying edits.
//!
//! A side branch working on the raw document from step 1 (not a numbered rendering
//! step). Messages are written for people and agents: they name what was
//! received and what was expected.

use thiserror::Error;

/// Why an edit line could not be parsed or applied.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
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

#[cfg(test)]
#[path = "../tests/unit/error.rs"]
mod tests;
