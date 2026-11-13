//! Unit tests for `src/lib.rs`.

use super::*;
use proptest::prelude::*;

#[test]
fn new_has_only_kind_and_message() {
    let d = Diagnostic::new(Kind::Missing, "missing `dur`");
    assert_eq!(d.kind, Kind::Missing);
    assert_eq!(d.path, None);
    assert_eq!(d.message, "missing `dur`");
    assert_eq!(d.expected, None);
}

#[test]
fn syntax_is_a_syntax_kind() {
    assert_eq!(Diagnostic::syntax("bad").kind, Kind::Syntax);
}

#[test]
fn display_is_the_bare_message_without_path_or_expected() {
    let d = Diagnostic::new(Kind::UnknownName, "no layer with id `ghost`");
    assert_eq!(d.to_string(), "no layer with id `ghost`");
    let boxed: Box<dyn std::error::Error> = Box::new(d);
    assert_eq!(boxed.to_string(), "no layer with id `ghost`");
}

#[test]
fn display_joins_message_and_expected_form() {
    let d = Diagnostic::syntax("bad value `x`").expected("`<number>f`");
    assert_eq!(d.to_string(), "bad value `x`; expected `<number>f`");
}

#[test]
fn display_with_path_and_with_or_without_expected() {
    let d = Diagnostic::new(Kind::UnknownName, "unknown preset `zoom`").at("layers[1].in");
    assert_eq!(d.to_string(), "layers[1].in: unknown preset `zoom`");
    let d = d.expected("`fade`");
    assert_eq!(
        d.to_string(),
        "layers[1].in: unknown preset `zoom`; expected `fade`"
    );
}

#[test]
fn at_replaces_the_path() {
    let d = Diagnostic::syntax("bad").at("a").at("b");
    assert_eq!(d.path.as_deref(), Some("b"));
}

#[test]
fn expected_replaces_the_form() {
    let d = Diagnostic::syntax("bad").expected("a").expected("b");
    assert_eq!(d.expected.as_deref(), Some("b"));
}

#[test]
fn under_sets_a_missing_path() {
    let d = Diagnostic::syntax("bad value `x`")
        .expected("`<number>f`")
        .under("dur");
    assert_eq!(d.to_string(), "dur: bad value `x`; expected `<number>f`");
}

#[test]
fn under_joins_a_field_with_a_dot() {
    let d = Diagnostic::syntax("bad").at("fill").under("layers[1]");
    assert_eq!(d.path.as_deref(), Some("layers[1].fill"));
}

#[test]
fn under_joins_an_index_without_a_dot() {
    let d = Diagnostic::syntax("bad").at("[2]").under("anim.x");
    assert_eq!(d.path.as_deref(), Some("anim.x[2]"));
    let d = d.under("layers[0]");
    assert_eq!(d.path.as_deref(), Some("layers[0].anim.x[2]"));
}

#[test]
fn under_an_empty_prefix_keeps_the_path() {
    let d = Diagnostic::syntax("bad").at("dur").under("");
    assert_eq!(d.path.as_deref(), Some("dur"));
    assert_eq!(Diagnostic::syntax("bad").under("").path, None);
}

#[test]
fn under_keeps_kind_message_and_expected() {
    let d = Diagnostic::new(Kind::OutOfRange, "too big")
        .expected("a number from 0 to 1")
        .under("opacity");
    assert_eq!(d.kind, Kind::OutOfRange);
    assert_eq!(d.message, "too big");
    assert_eq!(d.expected.as_deref(), Some("a number from 0 to 1"));
}

#[test]
fn unknown_lists_the_allowed_names() {
    let d = Diagnostic::unknown("preset", "zoom", ["fade", "rise"]).at("layers[1].in");
    assert_eq!(d.kind, Kind::UnknownName);
    assert_eq!(
        d.to_string(),
        "layers[1].in: unknown preset `zoom`; expected one of fade, rise"
    );
}

#[test]
fn unknown_accepts_any_displayable_names() {
    let d = Diagnostic::unknown("key", "colour", ["fill", "at"].map(|k| format!("`{k}`")));
    assert_eq!(
        d.to_string(),
        "unknown key `colour`; expected one of `fill`, `at`"
    );
}

#[test]
fn every_kind_exits_2() {
    let kinds = [
        Kind::Syntax,
        Kind::UnknownName,
        Kind::OutOfRange,
        Kind::Missing,
        Kind::Conflict,
        Kind::Invalid,
        Kind::Io,
    ];
    for kind in kinds {
        assert_eq!(kind.exit_code(), 2, "{kind:?}");
    }
}

proptest! {
    #[test]
    fn under_never_panics_and_keeps_both_parts(
        path in proptest::option::of(".*"),
        prefix in ".*",
    ) {
        let mut d = Diagnostic::syntax("bad");
        d.path = path.clone();
        let d = d.under(prefix.clone());
        let full = d.path.unwrap_or_default();
        prop_assert!(full.starts_with(&prefix));
        prop_assert!(full.ends_with(path.as_deref().unwrap_or_default()));
    }
}
