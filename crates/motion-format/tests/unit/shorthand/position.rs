//! Unit tests for `src/shorthand/position.rs`.

use super::*;
#[test]
fn position_parses_center_and_pixels() {
    let p = Position::parse("center 260").unwrap();
    assert_eq!((p.x, p.y), (Coord::Center, Coord::Px(260.0)));
    assert_eq!(Position::parse("100 -20").unwrap().y, Coord::Px(-20.0));
    assert_eq!(
        Position::parse("center 260").unwrap().to_string(),
        "center 260"
    );
    assert!(
        Position::parse("260")
            .unwrap_err()
            .to_string()
            .contains("`<x> <y>`, each a number of pixels or `center`, e.g. `center 260`")
    );
    assert!(Position::parse("left 3").is_err());
}
#[test]
fn position_display_round_trips_and_formats_numbers() {
    for text in ["center center", "100 -20", "12.5 0"] {
        assert_eq!(Position::parse(text).unwrap().to_string(), text);
    }
    assert_eq!(Position::parse("  1   2 ").unwrap().to_string(), "1 2");
}
#[test]
fn position_rejects_wrong_counts_and_non_finite_numbers() {
    for bad in ["", "1 2 3", "nan 1", "1 inf", "center,1"] {
        assert!(Position::parse(bad).is_err(), "{bad:?} should be rejected");
    }
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn position_any_string(s in ".*") {
        let _ = Position::parse(&s);
        }

        #[test]
        fn position_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = Position::parse(&s);
        }
    }
}

/// Property test: printing a position and parsing it back gives the same
/// position, for any pixel value that survives `fmt_num` (halves of integers).
mod round_trip {
    use super::*;
    use proptest::prelude::*;

    fn coord() -> impl Strategy<Value = Coord> {
        prop_oneof![
            Just(Coord::Center),
            (-20_000i32..20_000).prop_map(|n| Coord::Px(f64::from(n) / 2.0)),
        ]
    }

    proptest! {
        #[test]
        fn display_then_parse_is_identity(x in coord(), y in coord()) {
            let p = Position { x, y };
            prop_assert_eq!(Position::parse(&p.to_string()).unwrap(), p);
        }
    }
}
