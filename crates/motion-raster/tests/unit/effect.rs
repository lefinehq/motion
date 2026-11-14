//! Unit tests for `src/effect.rs`.

use super::*;

#[test]
fn every_effect_has_a_unique_name_and_documented_parameters() {
    for spec in EFFECTS {
        assert_eq!(EFFECTS.iter().filter(|s| s.name == spec.name).count(), 1);
        assert!(!spec.doc.is_empty(), "{}", spec.name);
        for param in spec.params {
            assert!(!param.doc.is_empty(), "{}.{}", spec.name, param.name);
            if let (ParamValue::Number(n), Some((lo, hi))) = (param.default, param.range) {
                assert!((lo..=hi).contains(&n), "{}.{}", spec.name, param.name);
            }
        }
    }
}

#[test]
fn left_out_parameters_take_their_defaults() {
    let fill = Effect::new("fill", &[]).unwrap();
    assert_eq!(fill.spec().name, "fill");
    assert_eq!(
        fill.values(),
        [
            ParamValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
            ParamValue::Number(1.0)
        ]
    );
    let blue = Effect::new(
        "fill",
        &[("color", ParamValue::Color(Color::rgba(0.0, 0.0, 1.0, 1.0)))],
    )
    .unwrap();
    assert_eq!(
        blue.values()[0],
        ParamValue::Color(Color::rgba(0.0, 0.0, 1.0, 1.0))
    );
    assert_eq!(blue.values()[1], ParamValue::Number(1.0));
}

#[test]
fn unknown_names_wrong_types_and_ranges_are_reported_at_the_parameter() {
    let e = Effect::new("glow", &[]).unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    assert_eq!(e.expected.as_deref(), Some("one of fill"));

    let e = Effect::new("fill", &[("colour", ParamValue::Number(1.0))]).unwrap_err();
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::UnknownName, Some("colour"))
    );
    assert_eq!(e.expected.as_deref(), Some("one of color, opacity"));

    let e = Effect::new("fill", &[("color", ParamValue::Number(1.0))]).unwrap_err();
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::WrongType, Some("color"))
    );
    assert_eq!(e.expected.as_deref(), Some("color"));

    let e = Effect::new("fill", &[("opacity", ParamValue::Number(1.5))]).unwrap_err();
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::OutOfRange, Some("opacity"))
    );
    assert_eq!(e.expected.as_deref(), Some("a number from 0 to 1"));
}

#[test]
fn values_know_their_type() {
    assert_eq!(ParamValue::Number(0.0).param_type(), ParamType::Number);
    assert_eq!(
        ParamValue::Color(Color::BLACK).param_type(),
        ParamType::Color
    );
}

#[test]
fn specs_compare_and_print_by_name() {
    let a = Effect::new("fill", &[]).unwrap();
    assert_eq!(a.spec(), &EFFECTS[0]);
    assert_eq!(format!("{:?}", a.spec()), r#"EffectSpec { name: "fill" }"#);
}
