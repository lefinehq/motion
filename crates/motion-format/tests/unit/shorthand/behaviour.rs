//! Unit tests for `src/shorthand/behaviour.rs`.

use super::*;
use crate::NAMED_CURVES;
use crate::units::Time;
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
    let e = "Fade-Up".parse::<Behaviour>().unwrap_err().to_string();
    assert!(e.contains("`Fade-Up` is not a valid preset name"));
    assert!(e.contains("ids use lowercase letters"));
    // Whether the preset exists is the scene parser's business, not this one's.
    assert!("fade-upp 12f".parse::<Behaviour>().is_ok());
    assert!(e.contains(
        "`<preset> [<n>px] [<n>f|<n>s] [<curve>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`; a curve is a name such as `ease-out`, `hold`, or `cubic(x1,y1,x2,y2)`"
    ));
    let e = "fade 12f 10f".parse::<Behaviour>().unwrap_err();
    assert_eq!(e.kind, Kind::Conflict);
    assert!(e.to_string().contains("duration given twice"));
    assert!(
        "fade bounce"
            .parse::<Behaviour>()
            .unwrap_err()
            .to_string()
            .contains("unknown curve `bounce`; expected one of linear, hold, ease")
    );
    // A misspelt curve gets the curve error even when it ends like a unit.
    let m = "fade easeout".parse::<Behaviour>().unwrap_err().message;
    assert!(m.contains("unknown curve `easeout`"), "{m}");
    let m = "fade ease-ins".parse::<Behaviour>().unwrap_err().message;
    assert!(m.contains("unknown curve `ease-ins`"), "{m}");
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
    for (text, what) in [
        ("fade 1px 2px", "distance"),
        ("fade linear ease-in", "curve"),
        ("fade @1f @2f", "start"),
    ] {
        let e = text.parse::<Behaviour>().unwrap_err().to_string();
        assert!(e.contains(&format!("{what} given twice")), "{text}: {e}");
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
    assert!(
        "".parse::<Behaviour>()
            .unwrap_err()
            .to_string()
            .contains("needs a preset")
    );
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
    let e = "fade 12f 10f".parse::<Behaviour>().unwrap_err().to_string();
    assert!(e.contains("duration given twice: `12f` and `10f`"), "{e}");
    let e = "fade linear ease-in"
        .parse::<Behaviour>()
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("curve given twice: `linear` and `ease-in`"),
        "{e}"
    );
}
#[test]
fn behaviour_value_errors_name_the_kind() {
    let msg = |t: &str| t.parse::<Behaviour>().unwrap_err().message;
    assert!(msg("fade 12x").contains("unknown token `12x`"));
    assert!(msg("fade -3f").contains("`-3f` is not a valid duration"));
    assert!(msg("fade xpx").contains("`xpx` is not a valid distance"));
    assert!(msg("fade @12").contains("`@12` is not a valid start time"));
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
fn cubic_easing_errors_show_the_expected_form() {
    let e = |t: &str| t.parse::<Behaviour>().unwrap_err().to_string();
    assert!(e("fade cubic(1.2,0,0,1)").contains("x1 and x2 must be between 0 and 1"));
    assert!(e("fade cubic(.2,0,1)").contains("`cubic(.2,0,1)` needs four numbers"));
    assert!(e("fade cubic(.2,0,0,a)").contains("needs four numbers"));
    assert!(e("fade cubic(.2,0,0,1").contains("missing its closing `)`"));
    assert!(e("fade cubic(2,0,0,1)").contains("e.g. `cubic(.2,0,0,1)`"));
    assert!(
        e("fade ease-in cubic(.2,0,0,1)")
            .contains("curve given twice: `ease-in` and `cubic(.2,0,0,1)`")
    );
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
    let msg = |t: &str| t.parse::<Behaviour>().unwrap_err().message;
    assert_eq!(msg("fade 1pxpx"), "`1pxpx` is not a valid distance");
    assert_eq!(msg("fade @12fx"), "`@12fx` is not a valid start time");
    assert_eq!(msg("fade 1.2.3s"), "`1.2.3s` is not a valid duration");
    assert!(msg("fade fades").starts_with("unknown curve `fades`"));
}

#[test]
fn behaviour_allows_any_whitespace_around_and_between_tokens() {
    let expected = "fade 12f".parse::<Behaviour>().unwrap();
    for text in ["  fade  12f  ", "fade\t12f", "fade\u{a0}12f"] {
        assert_eq!(text.parse::<Behaviour>().unwrap(), expected, "{text:?}");
    }
}
