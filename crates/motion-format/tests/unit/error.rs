//! Unit tests for `src/error.rs`.

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
