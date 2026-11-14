//! Unit tests for `src/curve.rs`.

use super::*;
use motion_diagnostic::Kind;

#[test]
fn every_table_name_parses_to_its_entry() {
    for (name, curve) in NAMED_CURVES {
        assert_eq!(name.parse::<Curve>().unwrap(), *curve, "{name}");
    }
}

#[test]
fn cubic_values_parse() {
    assert_eq!(
        "cubic(.2,0,0,1)".parse::<Curve>().unwrap(),
        Curve::Bezier(0.2, 0.0, 0.0, 1.0)
    );
    assert_eq!(
        "cubic(0.3, -0.5, 0.7, 1.6)".parse::<Curve>().unwrap(),
        Curve::Bezier(0.3, -0.5, 0.7, 1.6)
    );
}

#[test]
fn cubic_errors_show_the_expected_form() {
    let e = |t: &str| t.parse::<Curve>().unwrap_err().to_string();
    assert!(e("cubic(1.2,0,0,1)").contains("x1 and x2 must be between 0 and 1"));
    assert!(e("cubic(.2,0,1)").contains("`cubic(.2,0,1)` needs four numbers"));
    assert!(e("cubic(.2,0,0,1").contains("missing its closing `)`"));
    assert!(e("cubic(2,0,0,1)").contains("e.g. `cubic(.2,0,0,1)`"));
}

#[test]
fn unknown_name_lists_the_table() {
    assert_eq!("foo".parse::<Curve>().unwrap_err().kind, Kind::UnknownName);
    let e = "foo".parse::<Curve>().unwrap_err().to_string();
    assert_eq!(
        e,
        "unknown curve `foo`; expected one of linear, hold, ease, ease-in, ease-out, ease-in-out, or cubic(x1,y1,x2,y2)"
    );
    for (name, _) in NAMED_CURVES {
        assert!(e.contains(name));
    }
}

#[test]
fn display_uses_table_names_and_round_trips() {
    assert_eq!(
        "cubic(0,0,0.58,1)".parse::<Curve>().unwrap().to_string(),
        "ease-out"
    );
    assert_eq!(
        Curve::Bezier(0.2, 0.0, 0.0, 1.0).to_string(),
        "cubic(0.2,0,0,1)"
    );
    for (name, curve) in NAMED_CURVES {
        assert_eq!(curve.to_string(), *name);
    }
}

mod props {
    use super::*;
    use proptest::prelude::*;

    fn curves() -> impl Strategy<Value = Curve> {
        prop_oneof![
            proptest::sample::select(NAMED_CURVES).prop_map(|(_, c)| c),
            (0u32..=100, -200i32..=200, 0u32..=100, -200i32..=200).prop_map(|(a, b, c, d)| {
                Curve::Bezier(
                    a as f64 / 100.0,
                    b as f64 / 100.0,
                    c as f64 / 100.0,
                    d as f64 / 100.0,
                )
            }),
        ]
    }

    proptest! {
        #[test]
        fn display_round_trips(c in curves()) {
            prop_assert_eq!(c.to_string().parse::<Curve>().unwrap(), c);
        }

        #[test]
        fn parse_never_panics(s in ".*") {
            let _ = s.parse::<Curve>();
        }
    }
}

#[test]
fn every_non_bezier_curve_has_a_table_name() {
    // Display relies on this: it has no other way to name these curves.
    for curve in [Curve::Linear, Curve::Hold] {
        assert!(NAMED_CURVES.iter().any(|(_, c)| *c == curve), "{curve:?}");
    }
}

#[test]
fn cubic_expected_form_says_no_spaces() {
    let expected = "cubic(1,2)".parse::<Curve>().unwrap_err().expected.unwrap();
    assert!(expected.contains("(no spaces)"), "{expected}");
}

#[test]
fn cubic_closing_paren_must_end_the_token() {
    let message = |t: &str| t.parse::<Curve>().unwrap_err().message;
    assert_eq!(message("cubic(1,2))"), "`cubic(1,2))` needs four numbers");
    assert_eq!(
        message("cubic(0,0,1,1)x"),
        "`cubic(0,0,1,1)x` is missing its closing `)`"
    );
}
