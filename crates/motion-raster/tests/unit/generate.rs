//! Unit tests for `src/generate.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::style::Color;

const ONE: Space = Space {
    transform: Affine::IDENTITY,
    bounds: None,
    input: None,
};

/// A `width` by 1 opaque grey layer.
fn solid(width: u16, height: u16) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    pixels.data_mut().fill([0.5, 0.5, 0.5, 1.0]);
    pixels
}

fn color(r: f32, g: f32, b: f32) -> ParamValue {
    ParamValue::Color(Color::rgba(r, g, b, 1.0))
}

fn close(got: [f32; 4], want: [f32; 4]) {
    assert!(
        got.iter().zip(&want).all(|(g, w)| (g - w).abs() < 1e-3),
        "{got:?} is not {want:?}"
    );
}

use ParamValue::{Choice, Number, Point as At};

#[test]
fn a_linear_ramp_runs_from_start_to_end_and_holds_beyond() {
    let values = [
        At([0.0, 0.0]),
        color(0.0, 0.0, 0.0),
        At([10.0, 0.0]),
        color(1.0, 1.0, 1.0),
        Number(0.0),
        Choice(0),
    ];
    let mut pixels = solid(20, 1);
    ramp(&mut pixels, &values, &ONE);
    close(pixels.get(0, 0), [0.05, 0.05, 0.05, 1.0]);
    close(pixels.get(4, 0), [0.45, 0.45, 0.45, 1.0]);
    close(pixels.get(15, 0), [1.0; 4]);
}

#[test]
fn a_ramp_draws_only_where_the_layer_is_and_moves_with_it() {
    let values = [
        At([0.0, 0.0]),
        color(1.0, 0.0, 0.0),
        At([0.0, 10.0]),
        color(0.0, 0.0, 1.0),
        Number(0.0),
        Choice(1),
    ];
    let mut pixels = solid(10, 1);
    pixels.data_mut()[9] = [0.0; 4];
    // The layer sits 5 px right of the frame's origin.
    let space = Space {
        transform: Affine::translate((5.0, 0.0)),
        bounds: None,
        input: None,
    };
    ramp(&mut pixels, &values, &space);
    // Pixel 5's centre is (0.5, 0.5) in the layer, √0.5 px from `start`.
    let t = 0.5f32.sqrt() / 10.0;
    close(pixels.get(5, 0), [1.0 - t, 0.0, t, 1.0]);
    close(pixels.get(9, 0), [0.0; 4]);
}

#[test]
fn four_colors_hold_at_their_points_and_mix_between() {
    let values = [
        At([0.5, 0.5]),
        color(1.0, 0.0, 0.0),
        At([20.5, 0.5]),
        color(0.0, 1.0, 0.0),
        At([0.5, 20.5]),
        color(0.0, 0.0, 1.0),
        At([20.5, 20.5]),
        color(1.0, 1.0, 1.0),
        Number(100.0),
        Number(1.0),
        Choice(0),
    ];
    let mut pixels = solid(21, 21);
    four_color(&mut pixels, &values, &ONE);
    let corner = pixels.get(0, 0);
    assert!(corner[0] > 0.9 && corner[1] < 0.1, "{corner:?}");
    // At the middle, pixel centre (10.5, 10.5), every colour weighs the same.
    close(pixels.get(10, 10), [0.5, 0.5, 0.5, 1.0]);
}

#[test]
fn a_grid_draws_its_lines_or_its_cells() {
    let values = |draw: u8| {
        [
            At([0.0, 0.0]),
            At([0.0, 0.0]),
            Number(10.0),
            Number(10.0),
            Number(2.0),
            color(1.0, 0.0, 0.0),
            Number(1.0),
            Choice(1),
            Choice(draw),
            Choice(0),
        ]
    };
    let mut lines = solid(20, 20);
    grid(&mut lines, &values(0), &ONE);
    close(lines.get(10, 5), [1.0, 0.0, 0.0, 1.0]);
    close(lines.get(5, 5), [0.0; 4]);
    let mut cells = solid(20, 20);
    grid(&mut cells, &values(1), &ONE);
    close(cells.get(5, 5), [1.0, 0.0, 0.0, 1.0]);
    close(cells.get(10, 5), [0.0; 4]);
}

#[test]
fn venetian_blinds_hide_each_stripe_from_one_side() {
    let values = [Number(0.5), Number(0.0), Number(10.0), Number(0.0)];
    let mut pixels = solid(1, 20);
    venetian_blinds(&mut pixels, &values, &ONE);
    assert_eq!(pixels.get(0, 2)[3], 0.0);
    assert_eq!(pixels.get(0, 7)[3], 1.0);
    assert_eq!(pixels.get(0, 12)[3], 0.0);
}

fn noise_values(evolution: f32, seed: f32) -> Vec<ParamValue> {
    let mut values = vec![
        Number(1.0),
        Number(0.0),
        Number(0.0),
        Number(1.0),
        Number(1.0),
        Number(1.0),
        At([0.0, 0.0]),
        Number(6.0),
        Number(0.7),
        Number(0.56),
        Number(0.0),
        At([0.0, 0.0]),
        Number(evolution),
        Number(seed),
        Number(1.0),
    ];
    values.extend([Choice(0), Choice(0), Choice(0), Choice(0), Choice(1)]);
    values
}

#[test]
fn fractal_noise_is_grey_varied_and_moved_by_evolution_and_seed() {
    let render = |values: &[ParamValue]| {
        let mut pixels = solid(64, 64);
        fractal_noise(&mut pixels, values, &ONE);
        pixels
    };
    let base = render(&noise_values(0.0, 0.0));
    assert!(
        base.data()
            .iter()
            .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 1.0)
    );
    let levels: Vec<f32> = base.data().iter().map(|p| p[0]).collect();
    let (low, high) = levels
        .iter()
        .fold((1.0f32, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    assert!(high - low > 0.2, "{low}..{high}");
    // The same settings give the same noise.
    assert_eq!(render(&noise_values(0.0, 0.0)), base);
    assert_ne!(render(&noise_values(90.0, 0.0)), base);
    assert_ne!(render(&noise_values(0.0, 7.0)), base);
}
