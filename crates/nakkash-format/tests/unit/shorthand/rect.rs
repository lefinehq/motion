//! Unit tests for `src/shorthand/rect.rs`.

use super::*;
#[test]
fn rect_parses_size_and_optional_radius() {
    assert_eq!(
        "300x400 r24".parse::<RectShape>().unwrap(),
        RectShape {
            width: 300.0,
            height: 400.0,
            radius: 24.0
        }
    );
    assert_eq!("240x8".parse::<RectShape>().unwrap().radius, 0.0);
    let e = "300".parse::<RectShape>().unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_deref()),
        (crate::Kind::Syntax, Some(EXPECTED))
    );
    assert!("0x10".parse::<RectShape>().is_err());
}
#[test]
fn rect_rejects_malformed_input() {
    for bad in [
        "",
        "10x",
        "x10",
        "10x-5",
        "10x10 24",
        "10x10 r-1",
        "10x10 r",
        "10x10 r1 r2",
        "axb",
    ] {
        assert!(
            bad.parse::<RectShape>().is_err(),
            "{bad:?} should be rejected"
        );
    }
}
#[test]
fn rect_accepts_fractions_and_zero_radius() {
    let r = "10.5x2 r0".parse::<RectShape>().unwrap();
    assert_eq!((r.width, r.height, r.radius), (10.5, 2.0, 0.0));
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn rect_any_string(s in ".*") {
        let _ = s.parse::<RectShape>();
        }

        #[test]
        fn rect_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = s.parse::<RectShape>();
        }
    }
}

#[test]
fn rect_allows_whitespace_around_and_between_its_parts() {
    assert_eq!(
        " 10x10  r2 ".parse::<RectShape>().unwrap(),
        RectShape {
            width: 10.0,
            height: 10.0,
            radius: 2.0
        }
    );
}
