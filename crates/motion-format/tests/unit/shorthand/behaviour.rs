//! Unit tests for `src/shorthand/behaviour.rs`.

use super::*;
use crate::NAMED_CURVES;
use crate::units::Time;

/// The reason the grammar gives for `text`; `None` when there is no word.
fn problem(text: &str) -> Option<Problem<'_>> {
    let mut reason = None;
    let _ = parse_all(text, behaviour, |given| {
        reason = given;
        Diagnostic::syntax("")
    });
    reason
}

/// Asserts that `text` fails with `kind` and the behaviour's form.
#[track_caller]
fn assert_fails(text: &str, kind: Kind) {
    let e = text.parse::<Behaviour>().unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_deref()),
        (kind, Some(EXPECTED)),
        "{text:?}"
    );
}

#[test]
fn behaviour_uses_defaults() {
    let b = "fade-up".parse::<Behaviour>().unwrap();
    assert_eq!(
        b,
        Behaviour {
            preset: "fade-up".to_string(),
            distance: 40.0,
            duration: Time::Frames(12.0),
            curve: Curve::named("ease-out").unwrap(),
            start: Time::Frames(0.0)
        }
    );
}

#[test]
fn behaviour_reads_every_token_in_any_order() {
    let b = "fade-up 60px 12f ease-out @15f"
        .parse::<Behaviour>()
        .unwrap();
    assert_eq!(
        (b.distance, b.duration, b.start),
        (60.0, Time::Frames(12.0), Time::Frames(15.0))
    );
    assert_eq!(
        "fade @0.5s linear 0.4s"
            .parse::<Behaviour>()
            .unwrap()
            .duration,
        Time::Seconds(0.4)
    );
}

#[test]
fn behaviour_rejects_unknown_and_repeated_tokens() {
    assert_fails("Fade-Up", Kind::Syntax);
    assert!(matches!(
        problem("Fade-Up"),
        Some(Problem::Preset("Fade-Up"))
    ));
    // Whether the preset exists is the scene parser's business, not this one's.
    assert!("fade-upp 12f".parse::<Behaviour>().is_ok());
    assert_fails("fade 12f 10f", Kind::Conflict);
    // A curve that is not one gets the curve's own error, which lists the names.
    for (text, curve) in [
        ("fade bounce", "bounce"),
        ("fade easeout", "easeout"),
        ("fade ease-ins", "ease-ins"),
    ] {
        let e = text.parse::<Behaviour>().unwrap_err();
        assert_eq!(e, curve.parse::<Curve>().unwrap_err(), "{text}");
        assert_eq!(e.kind, Kind::UnknownName);
    }
}

#[test]
fn behaviour_parses_every_preset_and_curve() {
    for name in ["fade", "fade-up", "my_preset2"] {
        assert_eq!(name.parse::<Behaviour>().unwrap().preset, name);
    }
    for (name, curve) in NAMED_CURVES {
        assert_eq!(
            format!("fade {name}").parse::<Behaviour>().unwrap().curve,
            *curve
        );
    }
}

#[test]
fn behaviour_order_does_not_matter() {
    let a = "fade-down 20px 0.3s linear @2f"
        .parse::<Behaviour>()
        .unwrap();
    let b = "fade-down @2f linear 0.3s 20px"
        .parse::<Behaviour>()
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.distance, 20.0);
}

#[test]
fn behaviour_rejects_each_repeated_kind() {
    for (text, field) in [
        ("fade 1px 2px", Field::Distance),
        ("fade 12f 10f", Field::Duration),
        ("fade linear ease-in", Field::Curve),
        ("fade @1f @2f", Field::Start),
    ] {
        assert_fails(text, Kind::Conflict);
        assert!(
            matches!(problem(text), Some(Problem::Twice(twice, ..)) if twice == field),
            "{text}"
        );
    }
}

#[test]
fn behaviour_rejects_empty_and_malformed_values() {
    for bad in [
        "",
        "   ",
        "fade @",
        "fade @12",
        "fade xpx",
        "fade -3f",
        "fade 1.2.3s",
        "fade nanpx",
        "fade-up fade-up",
    ] {
        assert!(
            bad.parse::<Behaviour>().is_err(),
            "{bad:?} should be rejected"
        );
    }
    assert_fails("", Kind::Syntax);
    assert!(problem("").is_none());
}

#[test]
fn behaviour_allows_negative_distance() {
    assert_eq!(
        "fade-up -10px".parse::<Behaviour>().unwrap().distance,
        -10.0
    );
}

#[test]
fn behaviour_repeat_error_shows_both_tokens() {
    assert!(matches!(
        problem("fade 12f 10f"),
        Some(Problem::Twice(Field::Duration, "12f", "10f"))
    ));
    assert!(matches!(
        problem("fade linear ease-in"),
        Some(Problem::Twice(Field::Curve, "linear", "ease-in"))
    ));
}

#[test]
fn behaviour_value_errors_name_the_kind() {
    assert_fails("fade 12x", Kind::Syntax);
    assert!(matches!(problem("fade 12x"), Some(Problem::Unknown("12x"))));
    for (text, field, token) in [
        ("fade -3f", Field::Duration, "-3f"),
        ("fade xpx", Field::Distance, "xpx"),
        ("fade @12", Field::Start, "@12"),
    ] {
        assert_fails(text, Kind::Syntax);
        assert!(
            matches!(problem(text), Some(Problem::Malformed(f, t)) if f == field && t == token),
            "{text}"
        );
    }
}

#[test]
fn cubic_easing_gives_full_control() {
    let b = "fade-up cubic(.2,0,0,1) 20f".parse::<Behaviour>().unwrap();
    assert_eq!(b.curve, Curve::Bezier(0.2, 0.0, 0.0, 1.0));
    // y may overshoot, for a spring past the target.
    let b = "fade cubic(0.3,-0.5,0.7,1.6)".parse::<Behaviour>().unwrap();
    assert_eq!(b.curve, Curve::Bezier(0.3, -0.5, 0.7, 1.6));
}

#[test]
fn cubic_easing_errors_are_the_curve_errors() {
    for curve in [
        "cubic(1.2,0,0,1)",
        "cubic(.2,0,1)",
        "cubic(.2,0,0,a)",
        "cubic(.2,0,0,1",
        "cubic(2,0,0,1)",
    ] {
        let e = format!("fade {curve}").parse::<Behaviour>().unwrap_err();
        assert_eq!(e, curve.parse::<Curve>().unwrap_err(), "{curve}");
    }
    assert!(matches!(
        problem("fade ease-in cubic(.2,0,0,1)"),
        Some(Problem::Twice(Field::Curve, "ease-in", "cubic(.2,0,0,1)"))
    ));
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn behaviour_any_string(s in ".*") {
        let _ = s.parse::<Behaviour>();
        }

        #[test]
        fn behaviour_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = s.parse::<Behaviour>();
        }
    }
}

#[test]
fn behaviour_token_shape_decides_the_kind_reported() {
    for (text, field) in [
        ("fade 1pxpx", Field::Distance),
        ("fade @12fx", Field::Start),
        ("fade 1.2.3s", Field::Duration),
    ] {
        assert!(
            matches!(problem(text), Some(Problem::Malformed(f, _)) if f == field),
            "{text}"
        );
    }
    assert!(matches!(problem("fade fades"), Some(Problem::Curve(_))));
}

#[test]
fn behaviour_allows_any_whitespace_around_and_between_tokens() {
    let expected = "fade 12f".parse::<Behaviour>().unwrap();
    for text in ["  fade  12f  ", "fade\t12f", "fade\u{a0}12f"] {
        assert_eq!(text.parse::<Behaviour>().unwrap(), expected, "{text:?}");
    }
}
