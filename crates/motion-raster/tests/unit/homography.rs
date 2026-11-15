//! Unit tests for `src/homography.rs`: composing, inverting and mapping.

use super::*;
use proptest::prelude::*;

/// A perspective map: a plane tilted towards a camera.
fn tilted() -> Homography {
    Homography::new([[1.2, 0.1, 30.0], [-0.2, 0.9, 12.0], [0.001, -0.0005, 1.0]])
}

/// Whether two points are within `eps` of each other.
fn close(a: Point, b: Point, eps: f64) -> bool {
    (a - b).hypot() <= eps
}

#[test]
fn an_affine_maps_as_kurbo_does_with_w_one() {
    let affine = Affine::translate((5.0, -2.0)) * Affine::rotate(0.7) * Affine::scale(1.5);
    let h = Homography::from(affine);
    let p = Point::new(3.0, 4.0);
    let [x, y, w] = h.map(p);
    assert_eq!(w, 1.0);
    assert!(close(Point::new(x, y), affine * p, 1e-12));
    assert!(close(h.project(p).unwrap(), affine * p, 1e-12));
}

#[test]
fn composing_maps_by_the_right_first() {
    let (a, b) = (
        Affine::rotate(0.3),
        Affine::translate((10.0, 0.0)) * Affine::scale(2.0),
    );
    let p = Point::new(1.0, 2.0);
    let composed = Homography::from(a) * Homography::from(b);
    assert!(close(composed.project(p).unwrap(), (a * b) * p, 1e-12));
    let h = tilted() * Homography::from(b);
    assert!(close(
        h.project(p).unwrap(),
        tilted().project(b * p).unwrap(),
        1e-9
    ));
}

#[test]
fn the_inverse_undoes_the_map_and_keeps_depth_as_its_reciprocal() {
    let h = tilted();
    let back = h.inverse().unwrap();
    let p = Point::new(40.0, -25.0);
    let [x, y, w] = h.map(p);
    let seen = Point::new(x / w, y / w);
    let [u, v, q] = back.map(seen);
    assert!(close(Point::new(u / q, v / q), p, 1e-9));
    assert!((q - 1.0 / w).abs() < 1e-12);
    let identity = h * back;
    for (r, row) in identity.rows().iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            let expected = if r == c { 1.0 } else { 0.0 };
            assert!((value - expected).abs() < 1e-12, "{identity:?}");
        }
    }
}

#[test]
fn a_singular_map_has_no_inverse() {
    // Every point onto one line, as a plane seen edge-on is.
    let edge_on = Homography::new([[0.0, 0.0, 5.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    assert_eq!(edge_on.inverse(), None);
    // Large entries alone do not make it singular.
    let far = Homography::from(Affine::translate((1e7, -1e7)));
    assert!(far.inverse().is_some());
}

#[test]
fn a_point_behind_is_not_seen() {
    let behind = Homography::new([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.01, 0.0, 1.0]]);
    assert!(behind.project(Point::new(-50.0, 0.0)).is_some());
    assert_eq!(behind.project(Point::new(-150.0, 0.0)), None);
    assert_eq!(behind.jacobian(Point::new(-150.0, 0.0)), None);
    assert_eq!(behind.magnification(Point::new(-150.0, 0.0)), 0.0);
}

#[test]
fn the_tangent_matches_the_map_to_first_order() {
    let h = tilted();
    let p = Point::new(20.0, 30.0);
    let tangent = h.tangent(p).unwrap();
    assert!(close(tangent * p, h.project(p).unwrap(), 1e-9));
    let e = 1e-3;
    let q = p + vello_cpu::kurbo::Vec2::new(e, -e);
    // The error of a first-order match falls with the square of the step.
    assert!(close(tangent * q, h.project(q).unwrap(), 1e-6));
}

#[test]
fn singular_values_of_a_rotation_and_a_squash() {
    let [a, b] = singular_values([0.0, -2.0, 2.0, 0.0]);
    assert!((a - 2.0).abs() < 1e-12 && (b - 2.0).abs() < 1e-12);
    let [a, b] = singular_values([3.0, 0.0, 0.0, 0.5]);
    assert!((a - 3.0).abs() < 1e-12 && (b - 0.5).abs() < 1e-12);
}

proptest! {
    #[test]
    fn inverting_twice_gives_the_map_back(
        m in prop::array::uniform9(-4.0f64..4.0),
    ) {
        let h = Homography::new([[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]]);
        prop_assume!(h.determinant().abs() > 1e-3);
        let twice = h.inverse().unwrap().inverse().unwrap();
        for (a, b) in h.rows().iter().flatten().zip(twice.rows().iter().flatten()) {
            prop_assert!((a - b).abs() < 1e-6);
        }
    }
}
