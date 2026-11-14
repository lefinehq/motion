//! Unit tests for `src/table.rs`.

use super::*;

const ROWS: &[(&str, u8)] = &[("a", 1), ("b", 2), ("a", 3)];

#[test]
fn lookup_finds_the_first_row_of_a_name() {
    assert_eq!(lookup(ROWS, "a"), Some(&("a", 1)));
    assert_eq!(lookup(ROWS, "c"), None);
}

#[test]
fn position_counts_from_the_first_row() {
    assert_eq!(position(ROWS, "b"), Some(1));
    assert_eq!(position(ROWS, "c"), None);
}

#[test]
fn names_follow_table_order() {
    assert_eq!(names(ROWS).collect::<Vec<_>>(), ["a", "b", "a"]);
    let owned = [("x".to_owned(), ()), ("y".to_owned(), ())];
    assert_eq!(names(&owned).collect::<Vec<_>>(), ["x", "y"]);
}
