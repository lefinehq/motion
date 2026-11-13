//! Unit tests for `src/units.rs`.

use super::*;

#[test]
fn time_parses_frames_and_seconds() {
    assert_eq!(Time::parse("12f").unwrap(), Time::Frames(12.0));
    assert_eq!(Time::parse("0.4s").unwrap(), Time::Seconds(0.4));
    assert_eq!(Time::parse("5s").unwrap().to_frames(30.0), 150.0);
    assert_eq!(Time::parse("12f").unwrap().to_frames(60.0), 12.0);
}
#[test]
fn time_rejects_bad_input_with_expected_form() {
    let e = Time::parse("12").unwrap_err();
    assert!(
        e.to_string()
            .contains("expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`")
    );
    assert!(Time::parse("-3f").is_err());
    assert!(Time::parse("f").is_err());
}
#[test]
fn color_parses_three_six_and_eight_digits() {
    assert_eq!(
        Color::parse("#FFF").unwrap(),
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        }
    );
    assert_eq!(
        Color::parse("#0B0D12").unwrap(),
        Color {
            r: 11,
            g: 13,
            b: 18,
            a: 255
        }
    );
    assert_eq!(Color::parse("#00000033").unwrap().a, 0x33);
    assert!(
        Color::parse("red")
            .unwrap_err()
            .to_string()
            .contains("`#RGB`, `#RRGGBB` or `#RRGGBBAA`")
    );
    assert!(Color::parse("#GGG").is_err());
}
#[test]
fn size_parses_presets_and_explicit_pixels() {
    assert_eq!(
        Size::parse("1080p").unwrap(),
        Size {
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(
        Size::parse("720p").unwrap(),
        Size {
            width: 1280,
            height: 720
        }
    );
    assert_eq!(
        Size::parse("4k").unwrap(),
        Size {
            width: 3840,
            height: 2160
        }
    );
    assert_eq!(
        Size::parse("square").unwrap(),
        Size {
            width: 1080,
            height: 1080
        }
    );
    assert_eq!(
        Size::parse("vertical").unwrap(),
        Size {
            width: 1080,
            height: 1920
        }
    );
    assert_eq!(
        Size::parse("640x360").unwrap(),
        Size {
            width: 640,
            height: 360
        }
    );
    assert!(Size::parse("0x360").is_err());
    assert!(
        Size::parse("big")
            .unwrap_err()
            .to_string()
            .contains("`720p`, `1080p`, `4k`, `square`, `vertical` or `<width>x<height>`")
    );
}
#[test]
fn time_rejects_non_finite_and_malformed_numbers() {
    for bad in ["", "nanf", "inff", "infs", "1.2.3s", "1 s", "12F", "s"] {
        assert!(Time::parse(bad).is_err(), "{bad:?} should be rejected");
    }
    assert_eq!(Time::parse("0f").unwrap(), Time::Frames(0.0));
}
#[test]
fn time_error_names_the_input() {
    assert!(Time::parse("12").unwrap_err().message.contains("`12`"));
}
#[test]
fn color_three_digits_double_each_digit() {
    assert_eq!(
        Color::parse("#F80").unwrap(),
        Color {
            r: 255,
            g: 136,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        Color::parse("#abc").unwrap(),
        Color::parse("#AABBCC").unwrap()
    );
}
#[test]
fn color_rejects_wrong_lengths_missing_hash_and_non_ascii() {
    for bad in [
        "", "#", "#12", "#1234", "#12345", "#1234567", "FFF", "#ééé", "#+12",
    ] {
        assert!(Color::parse(bad).is_err(), "{bad:?} should be rejected");
    }
}
#[test]
fn size_rejects_malformed_explicit_sizes() {
    for bad in [
        "", "640x", "x360", "640x0", "-1x5", "1.5x2", "640X360", "1x2x3",
    ] {
        assert!(Size::parse(bad).is_err(), "{bad:?} should be rejected");
    }
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn time_color_size_any_string(s in ".*") {
        let _ = Time::parse(&s);
        let _ = Color::parse(&s);
        let _ = Size::parse(&s);
        }

        #[test]
        fn time_color_size_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = Time::parse(&s);
        let _ = Color::parse(&s);
        let _ = Size::parse(&s);
        }
    }
}
