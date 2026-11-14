//! Unit tests for `src/units.rs`.

use super::*;
use motion_diagnostic::Kind;

#[test]
fn time_parses_frames_and_seconds() {
    assert_eq!("12f".parse::<Time>().unwrap(), Time::Frames(12.0));
    assert_eq!("0.4s".parse::<Time>().unwrap(), Time::Seconds(0.4));
    assert_eq!("5s".parse::<Time>().unwrap().to_frames(30.0), 150.0);
    assert_eq!("12f".parse::<Time>().unwrap().to_frames(60.0), 12.0);
}
#[test]
fn time_rejects_bad_input_with_expected_form() {
    let e = "12".parse::<Time>().unwrap_err();
    assert!(
        e.to_string()
            .contains("expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`")
    );
    assert!("-3f".parse::<Time>().is_err());
    assert!("f".parse::<Time>().is_err());
}
#[test]
fn color_parses_three_six_and_eight_digits() {
    assert_eq!(
        "#FFF".parse::<Color>().unwrap(),
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        }
    );
    assert_eq!(
        "#0B0D12".parse::<Color>().unwrap(),
        Color {
            r: 11,
            g: 13,
            b: 18,
            a: 255
        }
    );
    assert_eq!("#00000033".parse::<Color>().unwrap().a, 0x33);
    assert!(
        "red"
            .parse::<Color>()
            .unwrap_err()
            .to_string()
            .contains("`#RGB`, `#RRGGBB` or `#RRGGBBAA`")
    );
    assert!("#GGG".parse::<Color>().is_err());
}
#[test]
fn size_parses_presets_and_explicit_pixels() {
    assert_eq!(
        "1080p".parse::<Size>().unwrap(),
        Size {
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(
        "720p".parse::<Size>().unwrap(),
        Size {
            width: 1280,
            height: 720
        }
    );
    assert_eq!(
        "4k".parse::<Size>().unwrap(),
        Size {
            width: 3840,
            height: 2160
        }
    );
    assert_eq!(
        "square".parse::<Size>().unwrap(),
        Size {
            width: 1080,
            height: 1080
        }
    );
    assert_eq!(
        "vertical".parse::<Size>().unwrap(),
        Size {
            width: 1080,
            height: 1920
        }
    );
    assert_eq!(
        "640x360".parse::<Size>().unwrap(),
        Size {
            width: 640,
            height: 360
        }
    );
    assert!("0x360".parse::<Size>().is_err());
    assert!(
        "big"
            .parse::<Size>()
            .unwrap_err()
            .to_string()
            .contains("one of 720p, 1080p, 4k, square, vertical, or <width>x<height>")
    );
}
#[test]
fn time_rejects_non_finite_and_malformed_numbers() {
    for bad in ["", "nanf", "inff", "infs", "1.2.3s", "1 s", "12F", "s"] {
        assert!(bad.parse::<Time>().is_err(), "{bad:?} should be rejected");
    }
    assert_eq!("0f".parse::<Time>().unwrap(), Time::Frames(0.0));
}
#[test]
fn time_error_names_the_input() {
    assert!("12".parse::<Time>().unwrap_err().message.contains("`12`"));
}
#[test]
fn color_three_digits_double_each_digit() {
    assert_eq!(
        "#F80".parse::<Color>().unwrap(),
        Color {
            r: 255,
            g: 136,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        "#abc".parse::<Color>().unwrap(),
        "#AABBCC".parse::<Color>().unwrap()
    );
}
#[test]
fn color_rejects_wrong_lengths_missing_hash_and_non_ascii() {
    for bad in [
        "", "#", "#12", "#1234", "#12345", "#1234567", "FFF", "#ééé", "#+12",
    ] {
        assert!(bad.parse::<Color>().is_err(), "{bad:?} should be rejected");
    }
}
#[test]
fn size_rejects_malformed_explicit_sizes() {
    for bad in [
        "", "640x", "x360", "640x0", "-1x5", "1.5x2", "640X360", "1x2x3",
    ] {
        assert!(bad.parse::<Size>().is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn unknown_size_lists_every_named_size_in_order() {
    let e = "8k".parse::<Size>().unwrap_err();
    assert_eq!(e.kind, Kind::UnknownName);
    let names: Vec<&str> = SIZES.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        e.to_string(),
        format!(
            "unknown size `8k`; expected one of {}, or <width>x<height>",
            names.join(", ")
        )
    );
    for (name, size) in SIZES {
        assert_eq!(name.parse::<Size>().unwrap(), *size);
    }
}

#[test]
fn malformed_explicit_size_gives_the_explicit_form() {
    for bad in ["640x", "640x0", "1x2x3", "+640x0"] {
        let e = bad.parse::<Size>().unwrap_err();
        assert_eq!(e.kind, Kind::Syntax, "{bad}");
        assert_eq!(
            e.to_string(),
            format!("`{bad}` is not a valid size; expected {EXPLICIT_EXPECTED}")
        );
    }
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn time_color_size_any_string(s in ".*") {
        let _ = s.parse::<Time>();
        let _ = s.parse::<Color>();
        let _ = s.parse::<Size>();
        }

        #[test]
        fn time_color_size_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = s.parse::<Time>();
        let _ = s.parse::<Color>();
        let _ = s.parse::<Size>();
        }
    }
}
