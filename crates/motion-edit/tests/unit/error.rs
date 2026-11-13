//! Unit tests for `src/error.rs`.

use super::*;

#[test]
fn display_is_the_bare_message() {
    let e = EditError::new("no layer with id `ghost`");
    assert_eq!(e.to_string(), "no layer with id `ghost`");
    let boxed: Box<dyn std::error::Error> = Box::new(e);
    assert_eq!(boxed.to_string(), "no layer with id `ghost`");
}
