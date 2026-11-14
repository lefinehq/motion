//! Unit tests for `src/shorthand/font.rs`.

use super::*;
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
    assert!(
        "Inter"
            .parse::<Font>()
            .unwrap_err()
            .to_string()
            .contains("`<family> <size> [<weight>]`, e.g. `Inter 96 700`")
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
    let e = "Baloo 2 120".parse::<Font>().unwrap_err().to_string();
    assert!(e.contains("`Baloo 2 120` is ambiguous"), "{e}");
    assert!(e.contains("e.g. `Baloo 2 120 400`"), "{e}");
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
    let msg = |t: &str| t.parse::<Font>().unwrap_err().message;
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
    let message = "1 2 3".parse::<Font>().unwrap_err().message;
    assert_eq!(message, "`1 2 3` has no font family");
}
