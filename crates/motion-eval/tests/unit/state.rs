//! Unit tests for `src/state.rs`.

use super::*;
use motion_format::property::PROPERTIES;

/// A 100x50 box at (500, 300) with the given scale and rotation.
fn state(scale: f64, rotate: f64) -> NodeState {
    let values = PROPERTIES
        .iter()
        .map(|p| match p.name {
            n if n == names::X => 500.0,
            n if n == names::Y => 300.0,
            n if n == names::SCALE => scale,
            n if n == names::ROTATE => rotate,
            _ => p.default,
        })
        .collect();
    NodeState {
        id: "n".into(),
        width: 100.0,
        height: 50.0,
        values,
    }
}

fn close(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) {
    let d = [a.0 - b.0, a.1 - b.1, a.2 - b.2, a.3 - b.3];
    assert!(d.iter().all(|v| v.abs() < 1e-9), "{a:?} vs {b:?}");
}

#[test]
fn bounds_of_a_plain_box() {
    assert_eq!(state(1.0, 0.0).bounds(), (450.0, 275.0, 550.0, 325.0));
}

#[test]
fn bounds_grow_with_scale() {
    close(state(2.0, 0.0).bounds(), (400.0, 250.0, 600.0, 350.0));
}

#[test]
fn a_negative_scale_mirrors_without_changing_the_area() {
    assert_eq!(state(-1.0, 0.0).bounds(), state(1.0, 0.0).bounds());
}

#[test]
fn a_quarter_turn_swaps_the_extents_exactly() {
    // Exact equality: the cos dust at 90 degrees must be snapped away.
    assert_eq!(state(1.0, 90.0).bounds(), (475.0, 250.0, 525.0, 350.0));
}

#[test]
fn a_45_degree_turn_uses_the_diagonal() {
    let half = (50.0 + 25.0) * std::f64::consts::FRAC_1_SQRT_2;
    close(
        state(1.0, 45.0).bounds(),
        (500.0 - half, 300.0 - half, 500.0 + half, 300.0 + half),
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
#[should_panic(expected = "no property `nope`")]
fn an_unknown_name_panics() {
    state(1.0, 0.0).get("nope");
}
