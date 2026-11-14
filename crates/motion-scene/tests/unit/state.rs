//! Unit tests for `src/state.rs`.

use super::*;

/// A 100x50 box at (500, 300) with the given scale and rotation.
fn state(scale: f64, rotate: f64) -> NodeState {
    let values = Prop::all()
        .map(|p| match p {
            Prop::X => 500.0,
            Prop::Y => 300.0,
            Prop::SCALE => scale,
            Prop::ROTATE => rotate,
            _ => p.spec().default,
        })
        .collect();
    NodeState {
        id: "n".into(),
        width: 100.0,
        height: 50.0,
        values,
    }
}

fn close(a: Rect, b: Rect) {
    let d = [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1];
    assert!(d.iter().all(|v| v.abs() < 1e-9), "{a:?} vs {b:?}");
}

#[test]
fn bounds_of_a_plain_box() {
    assert_eq!(
        state(1.0, 0.0).bounds(),
        Rect::new(450.0, 275.0, 550.0, 325.0)
    );
}

#[test]
fn bounds_grow_with_scale() {
    close(
        state(2.0, 0.0).bounds(),
        Rect::new(400.0, 250.0, 600.0, 350.0),
    );
}

#[test]
fn a_negative_scale_mirrors_without_changing_the_area() {
    assert_eq!(state(-1.0, 0.0).bounds(), state(1.0, 0.0).bounds());
}

#[test]
fn a_quarter_turn_swaps_the_extents_exactly() {
    // Exact equality: the cos dust at 90 degrees must be snapped away.
    assert_eq!(
        state(1.0, 90.0).bounds(),
        Rect::new(475.0, 250.0, 525.0, 350.0)
    );
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
fn get_reads_the_value_of_a_property() {
    let s = state(2.0, 45.0);
    assert_eq!((s.get(Prop::SCALE), s.get(Prop::ROTATE)), (2.0, 45.0));
    assert_eq!(s.get(Prop::OPACITY), 1.0);
}
