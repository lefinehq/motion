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
        outline: None,
        totals: Vec::new(),
        draft: false,
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

/// A 64 by 64 box at the frame's origin whose red is its pixel's column
/// over 64 and green its row over 64, so reading between pixels reads
/// the point exactly.
fn ramps() -> (Pixels, Space) {
    let mut pixels = Pixels::transparent(64, 64);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = ((i % 64) as f32, (i / 64) as f32);
        *p = [(x + 0.5) / 64.0, (y + 0.5) / 64.0, 0.0, 1.0];
    }
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: Some(Rect::new(0.0, 0.0, 64.0, 64.0)),
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
    };
    (pixels, space)
}

/// The point of the box `ramps` reads at pixel `x`, `y`.
fn read_at(pixels: &Pixels, x: i64, y: i64) -> [f32; 2] {
    let [r, g, ..] = pixels.get(x, y);
    [r * 64.0, g * 64.0]
}

fn near(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

#[test]
fn polar_coordinates_wraps_rows_into_rings_and_back() {
    // Rect to polar: the pixel straight left of the middle, 8 px away,
    // shows the point a quarter of the way across and 8√2 down.
    let (mut pixels, space) = ramps();
    polar_coordinates(&mut pixels, &[Number(1.0), Choice(0)], &space);
    let left = read_at(&pixels, 23, 31);
    let r = 8.5f32.hypot(0.5);
    let quarter = (8.5f32).atan2(0.5) / std::f32::consts::TAU;
    assert!(
        near(left, [64.0 * quarter, std::f32::consts::SQRT_2 * r]),
        "{left:?}"
    );
    // Just left of straight below: a little short of half way across.
    let below = read_at(&pixels, 31, 40);
    let r = 0.5f32.hypot(8.5);
    let short = 0.5 - 0.5f32.atan2(8.5) / std::f32::consts::TAU;
    assert!(
        near(below, [64.0 * short, std::f32::consts::SQRT_2 * r]),
        "{below:?}"
    );
    // Polar to rect: the pixel at column x and row y shows the point at
    // angle x/64 of a turn and y/√2 px from the middle.
    let (mut pixels, space) = ramps();
    polar_coordinates(&mut pixels, &[Number(1.0), Choice(1)], &space);
    let at = read_at(&pixels, 16, 20);
    let (angle, radius) = (
        std::f32::consts::TAU * 16.5 / 64.0,
        20.5 / std::f32::consts::SQRT_2,
    );
    assert!(
        near(
            at,
            [32.0 - radius * angle.sin(), 32.0 - radius * angle.cos()]
        ),
        "{at:?}"
    );
    // Halfway, a pixel reads halfway between itself and the converted point.
    let (mut pixels, space) = ramps();
    polar_coordinates(&mut pixels, &[Number(0.5), Choice(1)], &space);
    let half = read_at(&pixels, 16, 20);
    assert!(
        near(half, [(16.5 + at[0]) / 2.0, (20.5 + at[1]) / 2.0]),
        "{half:?}"
    );
    // At 0 nothing moves.
    let (mut pixels, space) = ramps();
    let before = pixels.clone();
    polar_coordinates(&mut pixels, &[Number(0.0), Choice(0)], &space);
    assert_eq!(pixels, before);
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
fn repeat_tile_repeats_flips_or_bricks_the_box_around_it() {
    let [red, green, blue, white] = [
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [1.0; 4],
    ];
    // A 4 by 1 box at x 4 to 8 of a 12 by 3 frame, one colour per pixel.
    let drawn = || {
        let mut pixels = Pixels::transparent(12, 3);
        pixels.data_mut()[4..8].copy_from_slice(&[red, green, blue, white]);
        let space = Space {
            transform: Affine::IDENTITY,
            bounds: Some(Rect::new(4.0, 0.0, 8.0, 1.0)),
            input: None,
            time: 0.0,
            sound: None,
            outline: None,
            totals: Vec::new(),
            draft: false,
        };
        (pixels, space)
    };
    let tiled = |tiling: u8| {
        let (mut pixels, space) = drawn();
        let values = [
            Number(4.0),
            Number(4.0),
            Number(1.0),
            Number(0.0),
            Choice(tiling),
        ];
        repeat_tile(&mut pixels, &values, &space);
        let row = |y| (0..12).map(|x| pixels.get(x, y)).collect::<Vec<_>>();
        (row(0), row(1), row(2))
    };
    let (top, below, beyond) = tiled(0);
    assert_eq!(top, [red, green, blue, white].repeat(3));
    assert_eq!(below, top);
    // Nothing past how far the tiles reach.
    assert_eq!(beyond, [[0.0; 4]; 12]);
    // Unfolded, the columns either side of the box flip across.
    let (top, ..) = tiled(1);
    let flipped = [white, blue, green, red];
    assert_eq!(top, [flipped, [red, green, blue, white], flipped].concat());
    // Bricked, the row below shifts by half a tile.
    let (top, below, _) = tiled(2);
    assert_eq!(top, [red, green, blue, white].repeat(3));
    assert_eq!(below, [blue, white, red, green].repeat(3));
    // With none, the box alone.
    let (top, below, _) = tiled(3);
    let empty = [[0.0; 4]; 4];
    assert_eq!(top, [empty, [red, green, blue, white], empty].concat());
    assert_eq!(below, [[0.0; 4]; 12]);
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
        outline: None,
        totals: Vec::new(),
        draft: false,
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
    space.input = Some(Input::new(map, Some(Rect::new(0.0, 0.0, 4.0, 1.0))));
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

/// The values of a lens of `degrees` about `center`, removing the lens
/// or adding it, over a `field` (0 width, 1 height, 2 diagonal), growing
/// as `grow` says (0 off, 1 twice, 2 four times).
fn lens(degrees: f32, center: [f32; 2], remove: bool, field: u8, grow: u8) -> [ParamValue; 5] {
    [
        Number(degrees),
        At(center),
        Choice(u8::from(remove)),
        Choice(field),
        Choice(grow),
    ]
}

#[test]
fn a_lens_of_no_field_bends_nothing() {
    let (mut pixels, space) = ramps();
    let before = pixels.clone();
    lens_distortion(&mut pixels, &lens(0.0, [32.0, 32.0], true, 0, 0), &space);
    assert_eq!(pixels, before);
}

#[test]
fn removing_a_lens_reads_each_point_nearer_its_axis() {
    // A 90° field across the 64 px width puts the focal length at 32 px:
    // the point 32 px out on the axis's row reads the point 32/√2 out.
    let (mut pixels, space) = ramps();
    lens_distortion(&mut pixels, &lens(90.0, [32.0, 32.5], true, 0, 0), &space);
    let focal = 32.0f32;
    for x in [40, 50, 63] {
        let r = x as f32 + 0.5 - 32.0;
        let expected = 32.0 + r / (1.0 + (r / focal).powi(2)).sqrt();
        let read = read_at(&pixels, x, 32);
        assert!(near(read, [expected, 32.5]), "{x}: {read:?} {expected}");
    }
    // The axis itself stays put.
    assert!(near(read_at(&pixels, 31, 32), [31.5, 32.5]));
}

#[test]
fn adding_a_lens_reads_each_point_farther_out_and_nothing_past_its_focal_length() {
    // A 90° field across the 64 px box's diagonal: the focal length is
    // half the diagonal, 32√2 px, over tan 45°.
    let (mut pixels, space) = ramps();
    lens_distortion(&mut pixels, &lens(90.0, [0.0, 0.0], false, 2, 0), &space);
    let focal = 64.0f32.hypot(64.0) / 2.0;
    let at = [10.5f32, 20.5];
    let r = at[0].hypot(at[1]);
    let k = 1.0 / (1.0 - (r / focal).powi(2)).sqrt();
    assert!(near(read_at(&pixels, 10, 20), [at[0] * k, at[1] * k]));
    // The far corner lies past the focal length, and points just inside it
    // read beyond the box: both clear.
    assert_eq!(pixels.get(63, 63), [0.0; 4]);
    assert_eq!(pixels.get(40, 0), [0.0; 4]);
}

#[test]
fn a_lens_grows_the_box_only_as_far_as_its_cap() {
    let bounds = Rect::new(0.0, 0.0, 64.0, 32.0);
    // Off, or adding a lens about the middle, keeps the box.
    assert_eq!(
        lens_box(bounds, &lens(120.0, [32.0, 16.0], true, 0, 0)),
        bounds
    );
    assert_eq!(
        lens_box(bounds, &lens(120.0, [32.0, 16.0], false, 0, 1)),
        bounds
    );
    // Removing a lens whose field reaches the corners' right angle grows
    // the box to its cap: twice its size about its middle.
    let capped = lens_box(bounds, &lens(120.0, [32.0, 16.0], true, 0, 1));
    let cap = Rect::new(-32.0, -16.0, 96.0, 48.0);
    assert!(
        (capped.x0 - cap.x0).abs() < 1e-9
            && (capped.y0 - cap.y0).abs() < 1e-9
            && (capped.x1 - cap.x1).abs() < 1e-9
            && (capped.y1 - cap.y1).abs() < 1e-9,
        "{capped:?}"
    );
    // A narrow one grows it to where its corners go, under the cap.
    let grown = lens_box(bounds, &lens(40.0, [32.0, 16.0], true, 0, 2));
    let focal = 32.0 / 20f64.to_radians().tan();
    let r = 32f64.hypot(16.0);
    let k = 1.0 / (1.0 - (r / focal).powi(2)).sqrt();
    assert!((grown.x1 - (32.0 + 32.0 * k)).abs() < 1e-9, "{grown:?}");
    assert!((grown.y0 - (16.0 - 16.0 * k)).abs() < 1e-9, "{grown:?}");
}

#[test]
fn a_grown_lens_draws_in_the_grown_box() {
    // A 32 px box in a 64 px grid: removing a wide lens draws past the box.
    let (mut pixels, mut space) = ramps();
    space.bounds = Some(Rect::new(16.0, 16.0, 48.0, 48.0));
    let values = lens(120.0, [32.0, 32.0], true, 0, 1);
    let mut kept = pixels.clone();
    lens_distortion(&mut pixels, &values, &space);
    assert_ne!(pixels.get(8, 32), [0.0; 4]);
    lens_distortion(&mut kept, &lens(120.0, [32.0, 32.0], true, 0, 0), &space);
    assert_eq!(kept.get(8, 32), [0.0; 4]);
}

/// A 96 by 96 grid with a 64 px box from (16, 16), whose red is the
/// layer point's x over 96 and green its y over 96, clear around it.
fn framed() -> (Pixels, Space) {
    let mut pixels = Pixels::transparent(96, 96);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = ((i % 96) as f32 + 0.5, (i / 96) as f32 + 0.5);
        if (16.0..80.0).contains(&x) && (16.0..80.0).contains(&y) {
            *p = [x / 96.0, y / 96.0, 0.0, 1.0];
        }
    }
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: Some(Rect::new(16.0, 16.0, 80.0, 80.0)),
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
    };
    (pixels, space)
}

/// The layer point `framed` shows at pixel `x`, `y`.
fn framed_at(pixels: &Pixels, x: i64, y: i64) -> [f32; 2] {
    let [r, g, ..] = pixels.get(x, y);
    [r * 96.0, g * 96.0]
}

/// The values of a warp moving each of its twelve points by `moves`.
fn warp(moves: [[f32; 2]; 12]) -> [ParamValue; 12] {
    moves.map(At)
}

#[test]
fn a_warp_at_rest_keeps_the_layer() {
    let (mut pixels, space) = framed();
    let before = pixels.clone();
    bezier_warp(&mut pixels, &warp([[0.0; 2]; 12]), &space);
    for (after, before) in pixels.data().iter().zip(before.data()) {
        assert!(
            after.iter().zip(before).all(|(a, b)| (a - b).abs() < 1e-4),
            "{after:?} {before:?}"
        );
    }
}

#[test]
fn a_warp_moving_every_point_alike_moves_the_layer() {
    let (mut pixels, space) = framed();
    bezier_warp(&mut pixels, &warp([[8.0, 4.0]; 12]), &space);
    assert!(near(framed_at(&pixels, 40, 40), [32.5, 36.5]));
    assert_eq!(pixels.get(18, 40), [0.0; 4]);
}

#[test]
fn a_warp_bends_a_side_through_its_handles() {
    // Both handles of the top side 12 px up: its middle rises 9 px, to
    // y 7, and the box's top row shows there.
    let (mut pixels, space) = framed();
    let mut moves = [[0.0; 2]; 12];
    moves[1] = [0.0, -12.0];
    moves[2] = [0.0, -12.0];
    bezier_warp(&mut pixels, &warp(moves), &space);
    let [r, g, _, a] = pixels.get(47, 7);
    assert!(a > 0.99, "{a}");
    assert!((r / a * 96.0 - 47.5).abs() < 0.05 && (g / a * 96.0 - 16.5).abs() < 0.05);
    assert_eq!(pixels.get(47, 5), [0.0; 4]);
    // Handles moved up and down move no point across.
    let [r, _, _, a] = pixels.get(16, 50);
    assert!((r / a * 96.0 - 16.5).abs() < 1e-3, "{}", r / a * 96.0);
}
