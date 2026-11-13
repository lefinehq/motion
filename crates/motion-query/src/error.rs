//! The error type of the query crate.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). Messages are written for people and agents: they say what was
//! wrong and what is accepted.

use thiserror::Error;

/// Why a query could not be answered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct QueryError {
    /// A message ready to show to the user.
    pub message: String,
}

impl QueryError {
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
