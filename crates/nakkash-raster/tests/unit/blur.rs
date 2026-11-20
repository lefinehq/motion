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
    outline: None,
    totals: Vec::new(),
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
        outline: None,
        totals: Vec::new(),
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

/// A grey bar with a white pixel in its middle, glowing by `composite`
/// and the operation called `operation`.
fn glow_by(composite: u8, operation: &str) -> Pixels {
    let mut pixels = Pixels::transparent(21, 1);
    pixels.data_mut().fill(GREY);
    pixels.data_mut()[10] = [1.0; 4];
    let at = GLOW_OPERATIONS
        .iter()
        .position(|o| *o == operation)
        .unwrap();
    let values = [
        ParamValue::Number(0.6),
        ParamValue::Number(10.0),
        ParamValue::Number(1.0),
        ParamValue::Choice(0),
        ParamValue::Choice(composite),
        ParamValue::Choice(u8::try_from(at).unwrap()),
    ];
    glow(&mut pixels, &values, &ONE);
    pixels
}

const GREY: Pixel = [0.4, 0.4, 0.4, 1.0];

#[test]
fn every_glow_operation_is_an_operation() {
    assert_eq!(GLOW_OPERATIONS[0], "add");
    for name in GLOW_OPERATIONS {
        assert!(Operation::named(name).is_ok(), "{name}");
    }
}

#[test]
fn a_glow_combines_with_its_layer_by_its_operation() {
    // Beside the white pixel its light is white at some alpha `a`.
    let alone = glow_by(1, "add").get(12, 0);
    let a = alone[3];
    assert!(a > 0.0 && a < 1.0 && alone[0] == a, "{alone:?}");
    let grey = GREY[0];
    // Laid over the grey: added, screened, or covering it by `a`.
    let near = |op: &str, composite: u8| glow_by(composite, op).get(12, 0)[0];
    assert!((near("add", 0) - (grey + a)).abs() < 1e-6);
    assert!((near("screen", 0) - (grey + a - grey * a)).abs() < 1e-6);
    assert!((near("normal", 0) - (grey + (1.0 - grey) * a)).abs() < 1e-6);
    // Behind the opaque grey, normal light is hidden; screen is the same
    // either way round.
    assert_eq!(glow_by(2, "normal").get(12, 0), GREY);
    assert!((near("screen", 2) - near("screen", 0)).abs() < 1e-6);
    // Alone, the light is all there is, whatever the operation.
    assert_eq!(glow_by(1, "screen").get(12, 0), alone);
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

/// An opaque 6 by 3 grid, black in its left half and white in its right.
fn step() -> Pixels {
    let mut pixels = Pixels::transparent(6, 3);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let v = if i % 6 < 3 { 0.0 } else { 1.0 };
        *p = [v, v, v, 1.0];
    }
    pixels
}

#[test]
fn edges_are_lines_where_the_colour_changes() {
    use ParamValue::{Choice, Number};
    // Light on dark: the two columns either side of the step light up
    // fully, the others stay black.
    let mut pixels = step();
    find_edges(&mut pixels, &[Number(0.0), Choice(1)], &ONE);
    let row: Vec<f32> = (0..6).map(|x| pixels.get(x, 1)[0]).collect();
    assert_eq!(row, [0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
    assert_eq!(pixels.get(2, 1)[3], 1.0);
    // Dark on light is the inverse, and the blend lets the layer through.
    let mut pixels = step();
    find_edges(&mut pixels, &[Number(0.5), Choice(0)], &ONE);
    let row: Vec<f32> = (0..6).map(|x| pixels.get(x, 1)[0]).collect();
    assert_eq!(row, [0.5, 0.5, 0.0, 0.5, 1.0, 1.0]);
}

#[test]
fn edges_keep_the_layers_alpha() {
    use ParamValue::{Choice, Number};
    // A clear pixel stays clear; the edge of an opaque one against it is
    // a change of colour.
    let mut pixels = impulse(3, 3, 1, 1);
    find_edges(&mut pixels, &[Number(0.0), Choice(1)], &ONE);
    assert_eq!(pixels.get(0, 0), [0.0; 4]);
    assert_eq!(pixels.get(1, 1), [0.0, 0.0, 0.0, 1.0]);
    let mut pixels = impulse(3, 3, 1, 1);
    find_edges(&mut pixels, &[Number(0.0), Choice(0)], &ONE);
    assert_eq!(pixels.get(1, 1), [1.0; 4]);
}

#[test]
fn a_blur_skips_only_lines_a_pass_leaves_as_they_are() {
    use crate::oracle::assert_same_bits;
    // Lines of +0.0 beside ones of -0.0 and ones crossing a speck.
    let (width, height) = (37usize, 23usize);
    let mut pixels = Pixels::transparent(width as u16, height as u16);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = (i % width, i / width);
        if y == 5 || x == 9 {
            *p = [-0.0; 4];
        }
        if (x, y) == (20, 15) {
            *p = [0.5, 0.25, 0.0, 1.0];
        }
    }
    for edges in [Edges::Transparent, Edges::Repeat] {
        for radius in [0.4f32, 1.0, 3.7] {
            let (whole, part) = (radius.floor() as usize, radius.fract());
            // The reference: every row, then every column, passed alone.
            let mut want = pixels.data().to_vec();
            let mut padded = Vec::new();
            for row in want.chunks_exact_mut(width) {
                for _ in 0..3 {
                    box_pass(row, whole, part, edges, &mut padded);
                }
            }
            for x in 0..width {
                let mut column: Vec<Pixel> = want.iter().skip(x).step_by(width).copied().collect();
                for _ in 0..3 {
                    box_pass(&mut column, whole, part, edges, &mut padded);
                }
                for (item, v) in want.iter_mut().skip(x).step_by(width).zip(column) {
                    *item = v;
                }
            }
            let mut got = pixels.clone();
            along(&mut got, 0, radius, 3, edges);
            along(&mut got, 1, radius, 3, edges);
            assert_same_bits(&format!("radius {radius}, {edges:?}"), &want, got.data());
        }
    }
}
