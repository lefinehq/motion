//! Unit tests for `src/blur.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::style::Color;

const ONE: Space = Space {
    transform: Affine::IDENTITY,
    bounds: None,
    input: None,
    time: 0.0,
    sound: None,
};

/// A `width` by `height` grid, transparent but for one opaque white pixel.
fn impulse(width: u16, height: u16, x: usize, y: usize) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    pixels.data_mut()[y * usize::from(width) + x] = [1.0; 4];
    pixels
}

/// The sum of alpha along row `y`, its mean x and its variance.
fn row_moments(pixels: &Pixels, y: usize) -> (f64, f64, f64) {
    let width = usize::from(pixels.width());
    let row = &pixels.data()[y * width..(y + 1) * width];
    let total: f64 = row.iter().map(|p| f64::from(p[3])).sum();
    let mean = row
        .iter()
        .enumerate()
        .map(|(x, p)| x as f64 * f64::from(p[3]))
        .sum::<f64>()
        / total;
    let variance = row
        .iter()
        .enumerate()
        .map(|(x, p)| (x as f64 - mean).powi(2) * f64::from(p[3]))
        .sum::<f64>()
        / total;
    (total, mean, variance)
}

#[test]
fn three_box_passes_have_the_gaussians_variance() {
    for sigma in [0.6f32, 1.0, 2.5, 7.3, 20.0] {
        let mut pixels = impulse(301, 1, 150, 0);
        gaussian(&mut pixels, [sigma, 0.0], Edges::Transparent);
        let (total, mean, variance) = row_moments(&pixels, 0);
        assert!((total - 1.0).abs() < 1e-4, "σ {sigma}: total {total}");
        assert!((mean - 150.0).abs() < 1e-3, "σ {sigma}: mean {mean}");
        let want = f64::from(sigma * sigma);
        assert!(
            (variance - want).abs() < want * 0.01 + 1e-3,
            "σ {sigma}: variance {variance}, want {want}"
        );
    }
}

#[test]
fn a_blur_along_rows_leaves_columns_alone() {
    let mut pixels = impulse(21, 21, 10, 10);
    let values = [
        ParamValue::Number(10.0),
        ParamValue::Choice(1),
        ParamValue::Choice(0),
    ];
    gaussian_blur(&mut pixels, &values, &ONE);
    assert!(pixels.get(10, 9)[3] == 0.0 && pixels.get(10, 11)[3] == 0.0);
    assert!(pixels.get(12, 10)[3] > 0.0);
}

#[test]
fn a_blur_scales_with_the_layer() {
    let values = [
        ParamValue::Number(10.0),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
    ];
    let mut once = impulse(201, 1, 100, 0);
    gaussian_blur(&mut once, &values, &ONE);
    let mut twice = impulse(201, 1, 100, 0);
    let doubled = Space {
        transform: Affine::scale(2.0),
        bounds: None,
        input: None,
        time: 0.0,
        sound: None,
    };
    gaussian_blur(&mut twice, &values, &doubled);
    let (_, _, v1) = row_moments(&once, 0);
    let (_, _, v2) = row_moments(&twice, 0);
    assert!((v2 / v1 - 4.0).abs() < 0.05, "{v1} then {v2}");
}

#[test]
fn repeated_edges_keep_a_solid_solid() {
    let mut pixels = Pixels::transparent(8, 8);
    pixels.data_mut().fill([0.2, 0.4, 0.6, 1.0]);
    let mut faded = pixels.clone();
    gaussian(&mut pixels, [3.0, 3.0], Edges::Repeat);
    assert!(
        pixels
            .data()
            .iter()
            .all(|p| (p[3] - 1.0).abs() < 1e-5 && (p[1] - 0.4).abs() < 1e-5)
    );
    gaussian(&mut faded, [3.0, 3.0], Edges::Transparent);
    assert!(faded.get(0, 0)[3] < 0.6);
}

#[test]
fn box_blur_runs_its_iterations() {
    let values = |iterations: f32| {
        [
            ParamValue::Number(2.0),
            ParamValue::Number(iterations),
            ParamValue::Choice(1),
            ParamValue::Choice(0),
        ]
    };
    let mut one = impulse(41, 1, 20, 0);
    box_blur(&mut one, &values(1.0), &ONE);
    // A box of radius 2 spreads one pixel over five.
    assert!((one.get(18, 0)[3] - 0.2).abs() < 1e-6 && one.get(17, 0)[3] == 0.0);
    let mut three = impulse(41, 1, 20, 0);
    box_blur(&mut three, &values(3.0), &ONE);
    let (_, _, variance) = row_moments(&three, 0);
    // Each box of radius 2 adds a variance of 2.
    assert!((variance - 6.0).abs() < 1e-3, "{variance}");
}

fn close(got: Pixel, want: Pixel) {
    assert!(
        got.iter().zip(&want).all(|(g, w)| (g - w).abs() < 1e-5),
        "{got:?} is not {want:?}"
    );
}

#[test]
fn a_drop_shadow_falls_where_its_direction_points() {
    let shadow = |direction: f32, layer: u8| {
        let values = [
            ParamValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
            ParamValue::Number(0.5),
            ParamValue::Number(direction),
            ParamValue::Number(3.0),
            ParamValue::Number(0.0),
            ParamValue::Choice(layer),
        ];
        let mut pixels = impulse(11, 11, 5, 5);
        drop_shadow(&mut pixels, &values, &ONE);
        pixels
    };
    // 90° is to the right; the layer stays over its shadow.
    let right = shadow(90.0, 0);
    close(right.get(8, 5), [0.5, 0.0, 0.0, 0.5]);
    assert_eq!(right.get(5, 5), [1.0; 4]);
    // 180° is down; alone, the layer is gone.
    let down = shadow(180.0, 1);
    close(down.get(5, 8), [0.5, 0.0, 0.0, 0.5]);
    assert_eq!(down.get(5, 5), [0.0; 4]);
}

#[test]
fn only_colours_past_the_threshold_glow() {
    let glow_of = |level: f32| {
        let mut pixels = Pixels::transparent(21, 1);
        pixels.data_mut()[10] = [level, level, level, 1.0];
        let values = [
            ParamValue::Number(0.6),
            ParamValue::Number(10.0),
            ParamValue::Number(1.0),
            ParamValue::Choice(0),
            ParamValue::Choice(0),
        ];
        glow(&mut pixels, &values, &ONE);
        pixels
    };
    assert_eq!(glow_of(0.5).get(12, 0), [0.0; 4]);
    let bright = glow_of(0.9);
    assert!(bright.get(12, 0)[3] > 0.0);
    // The original stays, its light added to.
    assert_eq!(bright.get(10, 0)[3], 1.0);
}

#[test]
fn sharpen_pushes_a_pixel_away_from_its_neighbours() {
    let mut pixels = Pixels::transparent(3, 3);
    pixels.data_mut().fill([0.5, 0.5, 0.5, 1.0]);
    pixels.data_mut()[4] = [0.6, 0.6, 0.6, 1.0];
    sharpen(&mut pixels, &[ParamValue::Number(100.0)], &ONE);
    assert!((pixels.get(1, 1)[0] - 0.7).abs() < 1e-6);
    // Its neighbour moves the other way; a corner beside only flat colour
    // stays as it is.
    assert!((pixels.get(1, 0)[0] - 0.475).abs() < 1e-6);
    assert!((pixels.get(0, 0)[0] - 0.5).abs() < 1e-6);
}
