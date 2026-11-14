//! Unit tests for `src/shorthand/position.rs`.

use super::*;

#[test]
fn coordinates_resolve_against_the_canvas() {
    assert_eq!(Coord::Center.resolve(1080.0), 540.0);
    assert_eq!(Coord::Px(-20.0).resolve(1080.0), -20.0);
    let p = "center 260".parse::<Position>().unwrap();
    assert_eq!(p.along(Axis::Y), Coord::Px(260.0));
    assert_eq!(p.resolve(Axis::X, (1920.0, 1080.0)), 960.0);
    assert_eq!(p.resolve(Axis::Y, (1920.0, 1080.0)), 260.0);
}

#[test]
fn position_parses_center_and_pixels() {
    let p = "center 260".parse::<Position>().unwrap();
    assert_eq!((p.x, p.y), (Coord::Center, Coord::Px(260.0)));
    assert_eq!("100 -20".parse::<Position>().unwrap().y, Coord::Px(-20.0));
    assert_eq!(
        "center 260".parse::<Position>().unwrap().to_string(),
        "center 260"
    );
    let e = "260".parse::<Position>().unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_deref()),
        (crate::Kind::Syntax, Some(EXPECTED))
    );
    assert!("left 3".parse::<Position>().is_err());
}
#[test]
fn position_display_round_trips_and_formats_numbers() {
    for text in ["center center", "100 -20", "12.5 0"] {
        assert_eq!(text.parse::<Position>().unwrap().to_string(), text);
    }
    assert_eq!("  1   2 ".parse::<Position>().unwrap().to_string(), "1 2");
}
#[test]
fn position_rejects_wrong_counts_and_non_finite_numbers() {
    for bad in ["", "1 2 3", "nan 1", "1 inf", "center,1"] {
        assert!(
            bad.parse::<Position>().is_err(),
            "{bad:?} should be rejected"
        );
    }
}

/// Property tests: the parsers must reject bad text with an error, never panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn position_any_string(s in ".*") {
        let _ = s.parse::<Position>();
        }

        #[test]
        fn position_short_printable_ascii(s in "[ -~]{0,40}") {
        let _ = s.parse::<Position>();
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
            prop_assert_eq!(p.to_string().parse::<Position>().unwrap(), p);
        }
    }
}
