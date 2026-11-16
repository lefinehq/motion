//! Unit tests for `src/distort.rs`.

use vello_cpu::kurbo::{Affine, Rect};

use crate::effect::Input;

use super::*;

use ParamValue::{Choice, Number, Point as At};

/// A 4 by 1 layer box drawn at the frame's origin, one colour per pixel.
fn stripes() -> (Pixels, Space) {
    let mut pixels = Pixels::transparent(8, 1);
    let colours = [
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [1.0; 4],
    ];
    pixels.data_mut()[..4].copy_from_slice(&colours);
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: Some(Rect::new(0.0, 0.0, 4.0, 1.0)),
        input: None,
        time: 0.0,
        sound: None,
    };
    (pixels, space)
}

#[test]
fn wrap_wraps_the_content_around_its_box() {
    let (mut pixels, space) = stripes();
    // The middle (2, 0.5) moves to (3, 0.5): everything one pixel right.
    wrap(&mut pixels, &[At([3.0, 0.5]), Number(0.0)], &space);
    assert_eq!(pixels.get(0, 0), [1.0; 4]);
    assert_eq!(pixels.get(1, 0), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(pixels.get(3, 0), [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn mirror_reflects_one_side_of_the_line_onto_the_other() {
    let [red, green, blue, white] = [
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [1.0; 4],
    ];
    // Upright through the middle: the right half shows the left.
    let (mut pixels, space) = stripes();
    mirror(&mut pixels, &[At([2.0, 0.5]), Number(0.0)], &space);
    let row: Vec<_> = (0..6).map(|x| pixels.get(x, 0)).collect();
    assert_eq!(row, [red, green, green, red, [0.0; 4], [0.0; 4]]);
    // Turned half a turn, the left half shows the right.
    let (mut pixels, space) = stripes();
    mirror(&mut pixels, &[At([2.0, 0.5]), Number(180.0)], &space);
    let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4);
    for (x, expected) in [white, blue, blue, white].into_iter().enumerate() {
        assert!(near(pixels.get(x as i64, 0), expected), "{x}");
    }
    // A line off the middle: what reaches the box's edge is reflected,
    // and what the layer drew outside its box does not show.
    let (mut pixels, space) = stripes();
    pixels.data_mut()[5] = white;
    mirror(&mut pixels, &[At([3.0, 0.5]), Number(0.0)], &space);
    let row: Vec<_> = (0..6).map(|x| pixels.get(x, 0)).collect();
    assert_eq!(row, [red, green, blue, blue, [0.0; 4], [0.0; 4]]);
}

#[test]
fn motion_tile_repeats_the_box_beyond_itself() {
    let (mut pixels, space) = stripes();
    let values = [
        At([2.0, 0.5]),
        Number(1.0),
        Number(1.0),
        Number(2.0),
        Number(1.0),
        Number(0.0),
        Choice(0),
        Choice(0),
    ];
    motion_tile(&mut pixels, &values, &space);
    // Twice as wide about the middle: from -2 to 6, the box repeated.
    assert_eq!(pixels.get(4, 0), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(pixels.get(5, 0), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(pixels.get(6, 0), [0.0; 4]);
    let (mut mirrored, space) = stripes();
    let mut values = values;
    values[6] = Choice(1);
    motion_tile(&mut mirrored, &values, &space);
    // The next tile flips: white first.
    assert_eq!(mirrored.get(4, 0), [1.0; 4]);
}

#[test]
fn turbulent_displacement_moves_pixels_but_keeps_pinned_edges() {
    let mut pixels = Pixels::transparent(64, 64);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let v = (i % 64) as f32 / 63.0;
        *p = [v, v, v, 1.0];
    }
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: Some(Rect::new(0.0, 0.0, 64.0, 64.0)),
        input: None,
        time: 0.0,
        sound: None,
    };
    let before = pixels.clone();
    let values = [
        Number(50.0),
        Number(16.0),
        At([0.0, 0.0]),
        Number(2.0),
        Number(0.0),
        Number(0.0),
        Choice(0),
        Choice(0),
    ];
    turbulent_displace(&mut pixels, &values, &space);
    assert_ne!(pixels, before);
    // At the edges the displacement fades to nearly nothing: half a pixel
    // in, it is 1/32 of what it is a cell in.
    let (edge, was) = (pixels.get(0, 31), before.get(0, 31));
    assert!(
        edge.iter().zip(&was).all(|(a, b)| (a - b).abs() < 0.05),
        "{edge:?}"
    );
    let none = [
        Number(0.0),
        Number(16.0),
        At([0.0, 0.0]),
        Number(2.0),
        Number(0.0),
        Number(0.0),
        Choice(0),
        Choice(3),
    ];
    let mut still = before.clone();
    turbulent_displace(&mut still, &none, &space);
    assert!(
        still
            .data()
            .iter()
            .zip(before.data())
            .all(|(a, b)| (a[0] - b[0]).abs() < 1e-5)
    );
}

#[test]
fn a_displacement_map_moves_pixels_by_its_levels() {
    let (mut pixels, mut space) = stripes();
    // A map all red: the most to the right. Green at 0.5: nothing down.
    let mut map = Pixels::transparent(8, 1);
    map.data_mut().fill([1.0, 0.5, 0.0, 1.0]);
    space.input = Some(Input {
        pixels: map,
        bounds: Some(Rect::new(0.0, 0.0, 4.0, 1.0)),
    });
    let values = [
        Number(1.0),
        Number(1.0),
        Choice(0),
        Choice(0),
        Choice(0),
        Choice(1),
    ];
    displacement_map(&mut pixels, &values, &space);
    // Each pixel reads the one to its right; with `wrap`, the last the first.
    assert_eq!(pixels.get(0, 0), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(pixels.get(2, 0), [1.0; 4]);
    assert_eq!(pixels.get(3, 0), [1.0, 0.0, 0.0, 1.0]);
}
