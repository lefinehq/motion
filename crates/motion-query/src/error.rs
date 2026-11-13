//! The error type of the query crate.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). Messages are written for people and agents: they say what was
//! wrong and what is accepted.

use std::fmt;

/// Why a query could not be answered.
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl fmt::Display for QueryError {
    /// Prints the bare message, with no prefix.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QueryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_the_bare_message() {
        let e = QueryError::new("no layer with id `ghost`");
        assert_eq!(e.to_string(), "no layer with id `ghost`");
        let boxed: Box<dyn std::error::Error> = Box::new(e);
        assert_eq!(boxed.to_string(), "no layer with id `ghost`");
    }
}
