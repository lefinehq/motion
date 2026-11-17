//! Unit tests for `src/grammar.rs`.

use super::*;
use crate::Kind;
use winnow::token::one_of;

/// Parses all of `text` with `grammar`, mapping any failure to one message.
fn all<'s, T>(text: &'s str, grammar: impl Parser<&'s str, T, ErrMode<Failure>>) -> Option<T> {
    parse_whole(text, grammar, "thing", "`<thing>`").ok()
}

#[test]
fn parse_whole_names_the_text_and_the_form() {
    let error = parse_whole("12x", number, "time", "`<n>f` or `<n>s`").unwrap_err();
    assert_eq!(
        (error.kind, error.path, error.expected.as_deref()),
        (Kind::Syntax, None, Some("`<n>f` or `<n>s`"))
    );
}

#[test]
fn parse_all_hands_the_innermost_reason_to_explain() {
    let grammar = word::<&str>.context("outer").try_map(|w: &str| match w {
        "ok" => Ok(()),
        _ => Err("inner"),
    });
    let error = parse_all("bad", grammar, |reason| {
        Diagnostic::syntax(reason.unwrap_or("none"))
    })
    .unwrap_err();
    assert_eq!(error.message, "inner");
}

#[test]
fn parse_at_hands_the_failing_offset_and_works_on_bytes() {
    let grammar = (b"ab".as_slice(), one_of(b'0'..=b'9').context("a digit"));
    let error = parse_at(b"abx".as_slice(), grammar, |reason, offset| {
        Diagnostic::syntax(format!("{} at {offset}", reason.unwrap_or("none")))
    })
    .unwrap_err();
    assert_eq!(error.message, "a digit at 2");
}

#[test]
fn numbers_are_finite_decimals() {
    assert_eq!(all("-0.5", number), Some(-0.5));
    assert_eq!(all(".2", number), Some(0.2));
    assert_eq!(all("1e3", number), Some(1000.0));
    for text in ["inf", "NaN", "1e999", "", "x"] {
        assert_eq!(all(text, number), None, "{text}");
    }
}

#[test]
fn words_and_blanks_split_on_whitespace() {
    assert_eq!(all("a\tb", (word, blank1, word)), Some(("a", "\t", "b")));
    assert_eq!(all("", blank0), Some(""));
    assert_eq!(all("", blank1), None);
    assert_eq!(all(" ", word), None);
}

#[test]
fn ends_with_returns_the_matched_suffix() {
    assert_eq!(all("12px", ends_with("px")), Some("px"));
    assert_eq!(all("delay+", ends_with(one_of(['+', '-']))), Some('+'));
    // The suffix must reach the end, not just appear.
    assert_eq!(all("a+b", ends_with(one_of(['+', '-']))), None);
}

/// Property tests: the shared pieces reject bad text, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn number_any_string(s in ".*") {
            let _ = all(&s, number);
        }

        #[test]
        fn ends_with_any_string(s in ".*") {
            let _ = all(&s, ends_with("px"));
        }

        #[test]
        fn words_any_string(s in ".*") {
            let _ = all(&s, (blank0, word, blank0));
        }
    }
}
