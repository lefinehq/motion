//! Unit tests for `src/lens.rs`.

use rstest::rstest;
use vello_cpu::kurbo::{Affine, Rect};

use super::*;
use crate::effect::Input;

const ONE: Space = Space {
    transform: Affine::IDENTITY,
    bounds: None,
    input: None,
    time: 0.0,
    sound: None,
    outline: None,
    totals: Vec::new(),
    draft: false,
};

/// A `width` by `height` grid, transparent but for one opaque white pixel.
fn impulse(width: u16, height: u16, x: usize, y: usize) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    pixels.data_mut()[y * usize::from(width) + x] = [1.0; 4];
    pixels
}

/// The values of `lens-blur`, in the order of its parameters.
fn values(radius: f32, sides: u8, roundness: f32, aspect: f32, edges: u8) -> [ParamValue; 10] {
    [
        ParamValue::Number(radius),
        ParamValue::Number(roundness),
        ParamValue::Number(aspect),
        ParamValue::Number(0.0),
        ParamValue::Number(0.0),
        ParamValue::Choice(match sides {
            6 => 0,
            n if n < 6 => n - 2,
            n => n - 3,
        }),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
        ParamValue::Choice(edges),
    ]
}

/// Whether the point lies inside the regular polygon of `sides` sides
/// with its corners on the circle of `radius`, one straight up.
fn in_polygon(x: f64, y: f64, sides: usize, radius: f64) -> bool {
    let wedge = TAU / sides as f64;
    let a = x.atan2(-y);
    let from_corner = (a + wedge / 2.0).rem_euclid(wedge) - wedge / 2.0;
    let edge = radius * (PI / sides as f64).cos() / from_corner.cos();
    x.hypot(y) <= edge
}

/// A single lit pixel spreads into the iris: lit inside it, dark outside,
/// and as much light as before.
#[rstest]
#[case::triangle(3)]
#[case::square(4)]
#[case::hexagon(6)]
#[case::decagon(10)]
fn a_point_spreads_into_the_iris(#[case] sides: u8) {
    let mut pixels = impulse(61, 61, 30, 30);
    lens_blur(&mut pixels, &values(20.0, sides, 0.0, 1.0, 0), &ONE);
    let total: f32 = pixels.data().iter().map(|p| p[3]).sum();
    assert!((total - 1.0).abs() < 1e-3, "{total}");
    for y in 0..61 {
        for x in 0..61 {
            let (dx, dy) = (x as f64 - 30.0, y as f64 - 30.0);
            let lit = pixels.get(x, y)[3] > 1e-6;
            // A px either side of the outline may go either way.
            if in_polygon(dx, dy, usize::from(sides), 19.0) {
                assert!(lit, "{sides} sides: ({x}, {y}) unlit");
            }
            if !in_polygon(dx, dy, usize::from(sides), 21.0) {
                assert!(!lit, "{sides} sides: ({x}, {y}) lit");
            }
        }
    }
}

#[test]
fn a_round_iris_is_a_disc() {
    let mut pixels = impulse(61, 61, 30, 30);
    lens_blur(&mut pixels, &values(20.0, 6, 1.0, 1.0, 0), &ONE);
    // On the diagonal, 19 px out, a hexagon of corner radius 20 has
    // already ended at its flat side for some angles; a disc has not.
    for (x, y) in [(43, 43), (17, 43), (30, 11), (11, 30)] {
        assert!(pixels.get(x, y)[3] > 1e-6, "({x}, {y})");
    }
    assert!(pixels.get(45, 45)[3] < 1e-6);
}

#[test]
fn a_wide_iris_reaches_further_across() {
    let mut pixels = impulse(81, 61, 40, 30);
    lens_blur(&mut pixels, &values(10.0, 10, 1.0, 2.0, 0), &ONE);
    assert!(pixels.get(57, 30)[3] > 1e-6 && pixels.get(40, 17)[3] < 1e-6);
}

#[test]
fn a_flat_layer_stays_flat_with_repeated_edges_and_fades_without() {
    let mut flat = Pixels::transparent(40, 30);
    for p in flat.data_mut() {
        *p = [0.2, 0.4, 0.6, 1.0];
    }
    let mut repeated = flat.clone();
    lens_blur(&mut repeated, &values(8.0, 6, 0.0, 1.0, 1), &ONE);
    for p in repeated.data() {
        for c in 0..4 {
            assert!((p[c] - flat.data()[0][c]).abs() < 1e-4, "{p:?}");
        }
    }
    let mut faded = flat;
    lens_blur(&mut faded, &values(8.0, 6, 0.0, 1.0, 0), &ONE);
    assert!(faded.get(0, 0)[3] < 0.5 && (faded.get(20, 15)[3] - 1.0).abs() < 1e-4);
}

/// The iris grows smoothly with its radius: a lit pixel's light at a
/// point moves a little for a little more radius, never in a jump.
#[test]
fn the_iris_grows_smoothly() {
    let lit = |radius: f32| {
        let mut pixels = impulse(41, 41, 20, 20);
        lens_blur(&mut pixels, &values(radius, 6, 0.0, 1.0, 0), &ONE);
        pixels.get(20, 31)[3]
    };
    let steps: Vec<f32> = (0..40).map(|k| lit(10.0 + k as f32 * 0.05)).collect();
    for w in steps.windows(2) {
        assert!((w[1] - w[0]).abs() < 0.002, "{steps:?}");
    }
}

/// With a map, a pixel's radius is `radius` times how far its level is
/// from the level in focus: the left half, at level 0, stays sharp with
/// focus at 0, and the right half, at level 1, spreads.
#[test]
fn a_blur_map_sets_the_radius_pixel_by_pixel() {
    let mut map = Pixels::transparent(60, 20);
    for (i, p) in map.data_mut().iter_mut().enumerate() {
        let level = if i % 60 < 30 { 0.0 } else { 1.0 };
        *p = [level, level, level, 1.0];
    }
    let mut pixels = Pixels::transparent(60, 20);
    pixels.data_mut()[10 * 60 + 10] = [1.0; 4];
    pixels.data_mut()[10 * 60 + 45] = [1.0; 4];
    let space = Space {
        input: Some(Input::new(map, Some(Rect::new(0.0, 0.0, 60.0, 20.0)))),
        bounds: Some(Rect::new(0.0, 0.0, 60.0, 20.0)),
        ..ONE
    };
    lens_blur(&mut pixels, &values(5.0, 6, 1.0, 1.0, 0), &space);
    assert!((pixels.get(10, 10)[3] - 1.0).abs() < 1e-4);
    assert!(pixels.get(11, 10)[3] < 1e-6);
    assert!(pixels.get(45, 10)[3] < 0.1 && pixels.get(48, 10)[3] > 1e-3);
}
