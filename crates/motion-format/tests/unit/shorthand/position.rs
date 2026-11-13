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
