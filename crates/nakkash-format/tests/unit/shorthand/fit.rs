//! Unit tests for `src/shorthand/fit.rs`.

use super::*;

#[test]
fn fit_parses_shrink_and_wrap_with_a_size_and_bare_shrink() {
    let parsed = |text: &str| text.parse::<TextFit>().unwrap();
    assert_eq!(
        parsed("shrink 600x300"),
        TextFit::Shrink {
            width: 600.0,
            height: 300.0
        }
    );
    assert_eq!(
        parsed(" wrap 1240.5x310 "),
        TextFit::Wrap {
            width: 1240.5,
            height: 310.0
        }
    );
    assert_eq!(parsed("shrink"), TextFit::ShrinkToBox);
    for written in ["shrink 600x300", "wrap 1240.5x310", "shrink"] {
        assert_eq!(parsed(written).written(), written);
    }
}

#[test]
fn fit_rejects_malformed_input() {
    for bad in [
        "",
        "grow",
        "wrap",
        "shrinks",
        "shrink600x300",
        "shrink 600",
        "shrink 0x300",
        "wrap 600x-1",
        "wrap 600x300 r2",
        "fill",
    ] {
        let e = bad.parse::<TextFit>().unwrap_err();
        assert_eq!(
            (e.kind, e.expected.as_deref()),
            (crate::Kind::Syntax, Some(EXPECTED)),
            "{bad:?}"
        );
    }
}
