//! Unit tests for `src/shorthand/font.rs`.

use super::*;
use crate::Kind;

/// The reason the grammar gives for `text`; `None` when nothing ends in a number.
fn problem(text: &str) -> Option<Problem<'_>> {
    let mut reason = None;
    let _ = parse_all(text, font, |given| {
        reason = given;
        Diagnostic::syntax("")
    });
    reason
}

#[test]
fn font_parses_family_with_spaces_size_and_weight() {
    assert_eq!(
        "Inter 96 700".parse::<Font>().unwrap(),
        Font {
            family: "Inter".into(),
            size: 96.0,
            weight: 700
        }
    );
    assert_eq!(
        "Noto Sans Arabic 40".parse::<Font>().unwrap(),
        Font {
            family: "Noto Sans Arabic".into(),
            size: 40.0,
            weight: 400
        }
    );
    let e = "Inter".parse::<Font>().unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_deref()),
        (Kind::Syntax, Some(EXPECTED))
    );
    assert!("Inter 96 950".parse::<Font>().is_err());
}
#[test]
fn font_size_without_weight_defaults_to_400() {
    let f = "Inter 96".parse::<Font>().unwrap();
    assert_eq!((f.family.as_str(), f.size, f.weight), ("Inter", 96.0, 400));
}
#[test]
fn font_weight_bounds_are_inclusive() {
    assert_eq!("A 10 100".parse::<Font>().unwrap().weight, 100);
    assert_eq!("A 10 900".parse::<Font>().unwrap().weight, 900);
    assert!("A 10 99".parse::<Font>().is_err());
}
#[test]
fn font_rejects_missing_family_and_bad_sizes() {
    for bad in [
        "",
        "96",
        "96 700",
        "Inter 0",
        "Inter -4",
        "Inter big",
        "Inter 96 700.5",
    ] {
        assert!(bad.parse::<Font>().is_err(), "{bad:?} should be rejected");
    }
}
#[test]
fn font_family_ending_in_a_number_needs_explicit_size_and_weight() {
    let f = "Baloo 2 120 400".parse::<Font>().unwrap();
    assert_eq!(
        (f.family.as_str(), f.size, f.weight),
        ("Baloo 2", 120.0, 400)
    );
}
#[test]
fn font_ambiguous_trailing_numbers_are_rejected_with_the_explicit_form() {
    assert!(matches!(
        problem("Baloo 2 120"),
        Some(Problem::Ambiguous { explicit }) if explicit == "Baloo 2 120"
    ));
    assert!("Inter 96 950".parse::<Font>().is_err());
    assert!("Inter 96 750".parse::<Font>().is_err());
}
#[test]
fn font_two_trailing_numbers_with_valid_weight_are_size_and_weight() {
    let f = "Baloo 2 700".parse::<Font>().unwrap();
    assert_eq!((f.family.as_str(), f.size, f.weight), ("Baloo", 2.0, 700));
}
#[test]
fn font_errors_name_their_cause() {
    assert!(problem("Inter").is_none(), "no size");
    assert!(matches!(problem("96 700"), Some(Problem::NoFamily)));
    assert!(matches!(problem("Inter 0"), Some(Problem::Size("0"))));
    assert!(matches!(
        problem("Inter 96 700 750"),
        Some(Problem::Weight("750"))
    ));
    for text in [
        "Inter",
        "96 700",
        "Inter 0",
        "Inter 96 700 750",
        "Baloo 2 120",
    ] {
        let e = text.parse::<Font>().unwrap_err();
        assert_eq!(
            (e.kind, e.expected.as_deref()),
            (Kind::Syntax, Some(EXPECTED))
        );
    }
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn font_any_string(s in ".*") {
        let _ = s.parse::<Font>();
        }

        #[test]
        fn font_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = s.parse::<Font>();
        }
    }
}

#[test]
fn font_ignores_surrounding_whitespace_and_checks_the_family_first() {
    let f = "  Inter   96  ".parse::<Font>().unwrap();
    assert_eq!((f.family.as_str(), f.size, f.weight), ("Inter", 96.0, 400));
    assert!(matches!(problem("1 2 3"), Some(Problem::NoFamily)));
}
