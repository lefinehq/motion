//! Unit tests for `src/contour.rs`.

use vello_cpu::kurbo::{Affine, Point};

use super::*;
use crate::style::Color;

use ParamValue::{Choice, Number};

/// A `size` by `size` level grid, 1 inside the squares `[x0, x1)` by
/// `[y0, y1)` of `blobs` and 0 elsewhere.
fn levels(size: usize, blobs: &[[usize; 4]]) -> Vec<f32> {
    (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            let inside = blobs
                .iter()
                .any(|[x0, y0, x1, y1]| (*x0..*x1).contains(&x) && (*y0..*y1).contains(&y));
            if inside { 1.0 } else { 0.0 }
        })
        .collect()
}

/// Twice the area a closed polyline winds, clockwise on the grid
/// positive.
fn winding(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

#[test]
fn a_square_traces_as_one_closed_clockwise_contour() {
    let contours = trace(&levels(12, &[[3, 3, 9, 9]]), 12, 12, 0.5, 0.0);
    assert_eq!(contours.len(), 1);
    let square = &contours[0];
    assert!(square.closed);
    // Its sides run between the pixel centres either side of the edge,
    // with each corner cut by half a pixel's diagonal.
    let expected = 4.0 * 5.0 + 4.0 * 0.5f64.sqrt();
    assert!(
        (square.length() - expected).abs() < 1e-9,
        "{}",
        square.length()
    );
    assert!(winding(&square.points) > 0.0);
    // Simplified, its straight runs keep only their ends.
    let simple = trace(&levels(12, &[[3, 3, 9, 9]]), 12, 12, 0.5, 0.1);
    assert_eq!(simple[0].points.len(), 8, "{:?}", simple[0].points);
}

#[test]
fn separate_shapes_trace_separately_from_the_top() {
    let contours = trace(
        &levels(16, &[[8, 1, 12, 4], [1, 8, 5, 12]]),
        16,
        16,
        0.5,
        0.0,
    );
    assert_eq!(contours.len(), 2);
    assert!(contours[0].points.iter().all(|p| p.y < 6.0));
    // A shape at the grid's edge closes along it.
    let edge = trace(&levels(8, &[[0, 0, 8, 3]]), 8, 8, 0.5, 0.0);
    assert_eq!(edge.len(), 1);
    assert!(edge[0].closed);
}

#[test]
fn a_stretch_of_a_closed_contour_carries_on_round_its_start() {
    let square = Contour {
        points: vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ],
        closed: true,
    };
    let piece = stretch(&square, 35.0, 45.0);
    assert_eq!(
        piece,
        [
            Point::new(0.0, 5.0),
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0)
        ]
    );
}

/// A 24 by 24 grid with an opaque white square from 6 to 18.
fn white_square() -> (Pixels, Space) {
    let mut pixels = Pixels::transparent(24, 24);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = (i % 24, i / 24);
        if (6..18).contains(&x) && (6..18).contains(&y) {
            *p = [1.0; 4];
        }
    }
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: None,
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
    };
    (pixels, space)
}

/// The values of red strokes, one round each contour at full opacity,
/// combined by `mode`.
fn strokes(mode: u8) -> Vec<ParamValue> {
    let spec = crate::effect::EFFECTS
        .iter()
        .find(|s| s.name == "contour-strokes")
        .expect("the row");
    let mut values: Vec<ParamValue> = spec.params.iter().map(|p| p.default).collect();
    values[4] = Number(1.0);
    values[8] = ParamValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0));
    values[9] = Number(3.0);
    values[10] = Number(1.0);
    values[14] = Number(1.0);
    values[20] = Choice(mode);
    values
}

#[test]
fn strokes_follow_the_outline_alone_or_over_the_layer() {
    let (mut pixels, space) = white_square();
    contour_strokes(&mut pixels, &strokes(0), &space);
    // On the outline, red; inside the square and far outside, nothing.
    assert_eq!(pixels.get(12, 6), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(pixels.get(12, 12), [0.0; 4]);
    assert_eq!(pixels.get(1, 1), [0.0; 4]);
    // Over the layer, the square shows inside the strokes.
    let (mut pixels, space) = white_square();
    contour_strokes(&mut pixels, &strokes(1), &space);
    assert_eq!(pixels.get(12, 6), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(pixels.get(12, 12), [1.0; 4]);
    // As a stencil, only the square's pixels under the strokes stay.
    let (mut pixels, space) = white_square();
    contour_strokes(&mut pixels, &strokes(3), &space);
    assert_eq!(pixels.get(12, 6), [1.0; 4]);
    assert_eq!(pixels.get(12, 12), [0.0; 4]);
}

#[test]
fn a_stroke_fades_from_its_start_to_its_end() {
    // The default opacity runs from 1 at a stroke's start to 0 at its
    // end: half way round the square it is half as opaque.
    let (mut pixels, space) = white_square();
    let mut values = strokes(0);
    values[14] = Number(0.0);
    contour_strokes(&mut pixels, &values, &space);
    let alphas: Vec<f32> = (6..18).map(|x| pixels.get(x, 6)[3]).collect();
    let bottom: Vec<f32> = (6..18).map(|x| pixels.get(x, 17)[3]).collect();
    // The contour starts at the top left and runs clockwise: the top side
    // is near its start, the bottom near its end.
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    assert!(mean(&alphas) > 0.6, "{alphas:?}");
    assert!(mean(&bottom) < 0.6 && mean(&bottom) > 0.2, "{bottom:?}");
}
