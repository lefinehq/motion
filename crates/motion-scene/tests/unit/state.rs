//! Unit tests for `src/state.rs`.

use super::*;
use rstest::rstest;

use motion_format::shorthand::{Coord, Position};

/// A 100x50 box centred at (500, 300) with the given scale and rotation.
fn state(scale: f64, rotate: f64) -> NodeState {
    let at = Position {
        x: Coord::Px(500.0),
        y: Coord::Px(300.0),
    };
    let values: Vec<Value> = Prop::all()
        .map(|p| match p.name() {
            "scale" => Value::Point([scale, scale]),
            "rotate" => Value::Number(rotate),
            _ => p.spec().rest(
                &at,
                kurbo::Rect::new(0.0, 0.0, 1920.0, 1080.0),
                kurbo::Rect::new(0.0, 0.0, 100.0, 50.0),
            ),
        })
        .collect();
    NodeState::of_box("n", 100.0, 50.0, values)
}

fn close(a: Rect, b: Rect) {
    let d = [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1];
    assert!(d.iter().all(|v| v.abs() < 1e-9), "{a:?} vs {b:?}");
}

/// Exact edges: a quarter turn must snap the cos dust at 90 degrees away.
#[rstest]
#[case::plain(1.0, 0.0, Rect::new(450.0, 275.0, 550.0, 325.0))]
#[case::scaled(2.0, 0.0, Rect::new(400.0, 250.0, 600.0, 350.0))]
#[case::quarter_turn_swaps_the_extents(1.0, 90.0, Rect::new(475.0, 250.0, 525.0, 350.0))]
fn bounds(#[case] scale: f64, #[case] rotate: f64, #[case] expected: Rect) {
    assert_eq!(state(scale, rotate).bounds(), expected);
}

#[test]
fn a_negative_scale_mirrors_without_changing_the_area() {
    assert_eq!(state(-1.0, 0.0).bounds(), state(1.0, 0.0).bounds());
}

#[test]
fn a_45_degree_turn_uses_the_diagonal() {
    let half = (50.0 + 25.0) * std::f64::consts::FRAC_1_SQRT_2;
    close(
        state(1.0, 45.0).bounds(),
        Rect::new(500.0 - half, 300.0 - half, 500.0 + half, 300.0 + half),
    );
}

#[test]
fn the_edge_getters_read_from_bounds() {
    let s = state(1.0, 0.0);
    assert_eq!(
        (s.left(), s.top(), s.right(), s.bottom()),
        (450.0, 275.0, 550.0, 325.0)
    );
}

#[test]
fn getters_read_the_value_of_a_property() {
    let s = state(2.0, 45.0);
    assert_eq!(s.get(Prop::SCALE), &Value::Point([2.0, 2.0]));
    assert_eq!(
        (s.point(Prop::SCALE), s.number(Prop::ROTATE)),
        ([2.0, 2.0], 45.0)
    );
    assert_eq!(s.number(Prop::OPACITY), 1.0);
}

#[test]
#[should_panic(expected = "a number property")]
fn number_rejects_a_point() {
    state(1.0, 0.0).number(Prop::SCALE);
}

#[test]
#[should_panic(expected = "a point property")]
fn point_rejects_a_number() {
    state(1.0, 0.0).point(Prop::ROTATE);
}
