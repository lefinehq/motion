//! Unit tests for `src/curve.rs`.

use super::*;

#[test]
fn every_table_name_parses_to_its_entry() {
    for (name, curve) in NAMED_CURVES {
        assert_eq!(Curve::parse(name).unwrap(), *curve, "{name}");
        assert!(Curve::is_curve_token(name));
    }
}

#[test]
fn cubic_values_parse() {
    assert_eq!(
        Curve::parse("cubic(.2,0,0,1)").unwrap(),
        Curve::Bezier(0.2, 0.0, 0.0, 1.0)
    );
    assert_eq!(
        Curve::parse("cubic(0.3, -0.5, 0.7, 1.6)").unwrap(),
        Curve::Bezier(0.3, -0.5, 0.7, 1.6)
    );
}

#[test]
fn cubic_errors_show_the_expected_form() {
    let e = |t: &str| Curve::parse(t).unwrap_err().to_string();
    assert!(e("cubic(1.2,0,0,1)").contains("x1 and x2 must be between 0 and 1"));
    assert!(e("cubic(.2,0,1)").contains("`cubic(.2,0,1)` needs four numbers"));
    assert!(e("cubic(.2,0,0,1").contains("missing its closing `)`"));
    assert!(e("cubic(2,0,0,1)").contains("e.g. `cubic(.2,0,0,1)`"));
}

#[test]
fn unknown_name_lists_the_table() {
    let e = Curve::parse("foo").unwrap_err().to_string();
    assert_eq!(
        e,
        "unknown curve `foo`; expected one of linear, hold, ease, ease-in, ease-out, ease-in-out, or cubic(x1,y1,x2,y2)"
    );
    for (name, _) in NAMED_CURVES {
        assert!(e.contains(name));
    }
}

#[test]
fn curve_tokens_are_classified() {
    assert!(Curve::is_curve_token("cubic(1,2"));
    assert!(!Curve::is_curve_token("bounce"));
    assert!(!Curve::is_curve_token("12f"));
}

#[test]
fn display_uses_table_names_and_round_trips() {
    assert_eq!(
        Curve::parse("cubic(0,0,0.58,1)").unwrap().to_string(),
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
            prop_assert_eq!(Curve::parse(&c.to_string()).unwrap(), c);
        }

        #[test]
        fn parse_never_panics(s in ".*") {
            let _ = Curve::parse(&s);
            let _ = Curve::is_curve_token(&s);
        }
    }
}
