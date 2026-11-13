//! Unit tests for `src/shorthand/font.rs`.

use super::*;
#[test]
fn font_parses_family_with_spaces_size_and_weight() {
    assert_eq!(
        Font::parse("Inter 96 700").unwrap(),
        Font {
            family: "Inter".into(),
            size: 96.0,
            weight: 700
        }
    );
    assert_eq!(
        Font::parse("Noto Sans Arabic 40").unwrap(),
        Font {
            family: "Noto Sans Arabic".into(),
            size: 40.0,
            weight: 400
        }
    );
    assert!(
        Font::parse("Inter")
            .unwrap_err()
            .to_string()
            .contains("`<family> <size> [<weight>]`, e.g. `Inter 96 700`")
    );
    assert!(Font::parse("Inter 96 950").is_err());
}
#[test]
fn font_size_without_weight_defaults_to_400() {
    let f = Font::parse("Inter 96").unwrap();
    assert_eq!((f.family.as_str(), f.size, f.weight), ("Inter", 96.0, 400));
}
#[test]
fn font_weight_bounds_are_inclusive() {
    assert_eq!(Font::parse("A 10 100").unwrap().weight, 100);
    assert_eq!(Font::parse("A 10 900").unwrap().weight, 900);
    assert!(Font::parse("A 10 99").is_err());
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
        assert!(Font::parse(bad).is_err(), "{bad:?} should be rejected");
    }
}
#[test]
fn font_family_ending_in_a_number_needs_explicit_size_and_weight() {
    let f = Font::parse("Baloo 2 120 400").unwrap();
    assert_eq!(
        (f.family.as_str(), f.size, f.weight),
        ("Baloo 2", 120.0, 400)
    );
}
#[test]
fn font_ambiguous_trailing_numbers_are_rejected_with_the_explicit_form() {
    let e = Font::parse("Baloo 2 120").unwrap_err().to_string();
    assert!(e.contains("`Baloo 2 120` is ambiguous"), "{e}");
    assert!(e.contains("e.g. `Baloo 2 120 400`"), "{e}");
    assert!(Font::parse("Inter 96 950").is_err());
    assert!(Font::parse("Inter 96 750").is_err());
}
#[test]
fn font_two_trailing_numbers_with_valid_weight_are_size_and_weight() {
    let f = Font::parse("Baloo 2 700").unwrap();
    assert_eq!((f.family.as_str(), f.size, f.weight), ("Baloo", 2.0, 700));
}
#[test]
fn font_errors_name_their_cause() {
    let msg = |t: &str| Font::parse(t).unwrap_err().message;
    assert!(msg("Inter").contains("has no size"));
    assert!(msg("96 700").contains("has no font family"));
    assert!(msg("Inter 0").contains("`0` is not a valid size"));
    assert!(msg("Inter 96 700 750").contains("weight must be 100, 200"));
    assert!(msg("Inter 96 700 750").contains("`750` is not a valid weight"));
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn font_any_string(s in ".*") {
        let _ = Font::parse(&s);
        }

        #[test]
        fn font_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = Font::parse(&s);
        }
    }
}
