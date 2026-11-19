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

#[test]
fn a_quad_map_takes_the_box_corners_to_the_quad() {
    let from = Rect::new(-20.0, 10.0, 180.0, 410.0);
    let to = [
        Point::new(300.0, 120.0),
        Point::new(520.0, 170.0),
        Point::new(500.0, 430.0),
        Point::new(290.0, 400.0),
    ];
    let map = Homography::from_quad(from, to).expect("a convex quad");
    let corners = [
        Point::new(from.x0, from.y0),
        Point::new(from.x1, from.y0),
        Point::new(from.x1, from.y1),
        Point::new(from.x0, from.y1),
    ];
    for (corner, target) in corners.into_iter().zip(to) {
        let seen = map.project(corner).expect("in front");
        assert!(close(seen, target, 1e-9), "{seen:?} for {target:?}");
    }
}

#[test]
fn a_quad_map_of_the_box_onto_itself_moves_nothing() {
    let from = Rect::new(0.0, 0.0, 300.0, 200.0);
    let to = [
        Point::new(0.0, 0.0),
        Point::new(300.0, 0.0),
        Point::new(300.0, 200.0),
        Point::new(0.0, 200.0),
    ];
    let map = Homography::from_quad(from, to).unwrap();
    for (row, want) in map.rows().iter().zip(Homography::IDENTITY.rows()) {
        for (v, w) in row.iter().zip(want) {
            assert!((v - w).abs() < 1e-12, "{:?}", map.rows());
        }
    }
}

#[test]
fn a_mirrored_quad_is_laid_in_front() {
    let from = Rect::new(0.0, 0.0, 10.0, 10.0);
    let to = [
        Point::new(10.0, 0.0),
        Point::new(0.0, 0.0),
        Point::new(0.0, 10.0),
        Point::new(10.0, 10.0),
    ];
    let map = Homography::from_quad(from, to).unwrap();
    assert!(map.map(Point::new(5.0, 5.0))[2] > 0.0);
    assert!(close(
        map.project(Point::new(2.0, 3.0)).unwrap(),
        Point::new(8.0, 3.0),
        1e-12
    ));
}

#[test]
fn crossed_bent_flat_and_sizeless_quads_have_no_map() {
    let from = Rect::new(0.0, 0.0, 10.0, 10.0);
    let quad = |q: [[f64; 2]; 4]| q.map(|[x, y]| Point::new(x, y));
    for to in [
        quad([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]]),
        quad([[0.0, 0.0], [10.0, 0.0], [3.0, 3.0], [0.0, 10.0]]),
        quad([[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [0.0, 10.0]]),
    ] {
        assert_eq!(Homography::from_quad(from, to), None, "{to:?}");
    }
    let square = quad([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    assert_eq!(
        Homography::from_quad(Rect::new(0.0, 0.0, 0.0, 10.0), square),
        None
    );
}

proptest! {
    /// Any convex quad near a box takes the box's corners there, and the
    /// box's middle inside it, in front.
    #[test]
    fn convex_quads_map_their_corners(
        jitter in prop::array::uniform8(-40.0f64..40.0),
        scale in 0.2f64..4.0,
    ) {
        let from = Rect::new(0.0, 0.0, 400.0, 300.0);
        let base = [(0.0, 0.0), (400.0, 0.0), (400.0, 300.0), (0.0, 300.0)];
        let to: [Point; 4] = std::array::from_fn(|k| {
            Point::new(
                100.0 + base[k].0 * scale + jitter[2 * k],
                50.0 + base[k].1 * scale + jitter[2 * k + 1],
            )
        });
        let Some(map) = Homography::from_quad(from, to) else {
            return Ok(());
        };
        let corners = [(0.0, 0.0), (400.0, 0.0), (400.0, 300.0), (0.0, 300.0)];
        for ((x, y), target) in corners.into_iter().zip(to) {
            let seen = map.project(Point::new(x, y)).unwrap();
            prop_assert!(close(seen, target, 1e-6));
        }
        prop_assert!(map.map(Point::new(200.0, 150.0))[2] > 0.0);
    }
}
