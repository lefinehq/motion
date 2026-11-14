//! Unit tests for `src/curve.rs`.

use super::*;
use crate::Kind;

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

/// The reason the grammar gives for `token`; `None` for an unknown name.
fn problem(token: &str) -> Option<Problem> {
    let mut reason = None;
    let _ = parse_all(token, curve, |given| {
        reason = given;
        Diagnostic::syntax("")
    });
    reason
}

#[test]
fn cubic_errors_name_their_cause_and_show_the_form() {
    for token in [
        "cubic(1.2,0,0,1)",
        "cubic(.2,0,1)",
        "cubic(.2,0,0,1",
        "cubic(2,0,0,1)",
    ] {
        let e = token.parse::<Curve>().unwrap_err();
        assert_eq!(
            (e.kind, e.expected.as_deref()),
            (Kind::Syntax, Some(CUBIC_EXPECTED)),
            "{token}"
        );
    }
    assert!(matches!(problem("cubic(1.2,0,0,1)"), Some(Problem::XRange)));
    assert!(matches!(problem("cubic(2,0,0,1)"), Some(Problem::XRange)));
    assert!(matches!(problem("cubic(.2,0,1)"), Some(Problem::Count)));
    assert!(matches!(problem("cubic(.2,0,0,1"), Some(Problem::Unclosed)));
}

#[test]
fn unknown_name_lists_the_table() {
    let e = "foo".parse::<Curve>().unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    assert!(problem("foo").is_none());
    let expected = e.expected.unwrap();
    for (name, _) in NAMED_CURVES {
        assert!(expected.contains(name), "{name}");
    }
    for form in CUSTOM_FORMS {
        assert!(expected.contains(form), "{expected}");
    }
}

#[test]
fn speed_reads_four_numbers_with_reaches_in_range() {
    assert_eq!(
        "speed(0,.33,-120.5,1)".parse::<Curve>().unwrap(),
        Curve::Speed {
            out_speed: 0.0,
            out_reach: 0.33,
            in_speed: -120.5,
            in_reach: 1.0
        }
    );
    for (bad, message) in [
        ("speed(0,.33,0)", "needs four numbers"),
        ("speed(0,.33,0,.33", "needs four numbers"),
        ("speed(0,1.5,0,.33)", "each reach must be between 0 and 1"),
        ("speed(0,.5,0,-0.1)", "each reach must be between 0 and 1"),
    ] {
        let e = bad.parse::<Curve>().unwrap_err();
        assert_eq!(e.kind, Kind::Syntax, "{bad}");
        assert!(e.message.contains(message), "{bad}: {e}");
        assert_eq!(e.expected.as_deref(), Some(SPEED_EXPECTED));
    }
}

#[test]
fn speed_displays_its_four_numbers_and_round_trips() {
    let curve = Curve::Speed {
        out_speed: 250.0,
        out_reach: 0.1667,
        in_speed: 0.0,
        in_reach: 0.75,
    };
    assert_eq!(curve.to_string(), "speed(250,0.17,0,0.75)");
    let exact = "speed(250,0.25,0,0.75)".parse::<Curve>().unwrap();
    assert_eq!(exact.to_string().parse::<Curve>().unwrap(), exact);
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
    assert!(matches!(problem("cubic(1,2))"), Some(Problem::Count)));
    assert!(matches!(
        problem("cubic(0,0,1,1)x"),
        Some(Problem::Unclosed)
    ));
}
