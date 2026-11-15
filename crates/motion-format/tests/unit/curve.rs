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

#[test]
fn every_shape_and_family_parses_and_prints_back() {
    for (shape_name, shape) in SHAPES {
        for (family_name, family) in FAMILIES {
            let token = format!("{shape_name}-{family_name}");
            let curve = token.parse::<Curve>().unwrap();
            assert_eq!(curve, Curve::Shaped(*shape, *family), "{token}");
            assert_eq!(curve.to_string(), token);
        }
    }
}

#[test]
fn family_parameters_parse_and_defaults_print_as_the_name() {
    let parse = |t: &str| t.parse::<Curve>().unwrap();
    assert_eq!(
        parse("in-out(2.5)"),
        Curve::Shaped(Shape::InOut, Family::Power(2.5))
    );
    assert_eq!(parse("in(2)").to_string(), "in-quad");
    assert_eq!(
        parse("out-back(2)"),
        Curve::Shaped(Shape::Out, Family::Back(2.0))
    );
    assert_eq!(parse("out-back(2)").to_string(), "out-back(2)");
    assert_eq!(
        parse("out-elastic(1.5)"),
        Curve::Shaped(
            Shape::Out,
            Family::Elastic {
                amplitude: 1.5,
                period: ELASTIC.1
            }
        )
    );
    assert_eq!(
        parse("in-out-elastic(2,.5)").to_string(),
        "in-out-elastic(2,0.5)"
    );
    assert_eq!(parse("out-in(3)").to_string(), "out-in-cubic");
    assert_eq!(parse("out(1.7)").to_string(), "out(1.7)");
}

#[test]
fn a_misshapen_family_names_the_family_form() {
    for token in [
        "in",
        "out-expo(2)",
        "in(0)",
        "out-back(-1)",
        "out-elastic(.5)",
        "out-elastic(1,3)",
        "in-out(1,2,3)",
        "out-quad(",
    ] {
        let e = token.parse::<Curve>().unwrap_err();
        assert_eq!(
            (e.kind, e.expected.as_deref()),
            (Kind::Syntax, Some(FAMILY_EXPECTED)),
            "{token}"
        );
    }
    // Only looking like one is an unknown name.
    for token in ["outback", "in-bouncy", "bounce", "out-expo-"] {
        assert_eq!(
            token.parse::<Curve>().unwrap_err().kind,
            Kind::UnknownName,
            "{token}"
        );
    }
}

#[test]
fn steps_parse_and_print() {
    let parse = |t: &str| t.parse::<Curve>().unwrap();
    assert_eq!(
        parse("steps(4)"),
        Curve::Steps {
            count: 4,
            start: false
        }
    );
    assert_eq!(parse("steps(3,start)").to_string(), "steps(3,start)");
    assert_eq!(parse("steps(1)").to_string(), "steps(1)");
    for token in ["steps(0)", "steps(2.5)", "steps(2,end)", "steps(2"] {
        let e = token.parse::<Curve>().unwrap_err();
        assert_eq!(e.expected.as_deref(), Some(STEPS_EXPECTED), "{token}");
    }
}
