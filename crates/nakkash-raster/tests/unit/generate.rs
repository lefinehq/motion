//! Unit tests for `src/generate.rs`.

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
    draft: false,
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
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
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

/// A 100 by 1 grey layer box, wiped `completion` of the way at `angle`
/// with `feather`: the alpha of every tenth pixel.
fn wiped(completion: f32, angle: f32, feather: f32) -> Vec<f32> {
    let mut pixels = solid(100, 1);
    let space = Space {
        bounds: Some(vello_cpu::kurbo::Rect::new(0.0, 0.0, 100.0, 1.0)),
        ..ONE
    };
    let values = [Number(completion), Number(angle), Number(feather)];
    linear_wipe(&mut pixels, &values, &space);
    (0..10).map(|i| pixels.get(i * 10 + 5, 0)[3]).collect()
}

#[test]
fn a_linear_wipe_at_90_degrees_hides_the_box_from_its_left() {
    assert_eq!(wiped(0.0, 90.0, 0.0), [1.0; 10]);
    assert_eq!(wiped(1.0, 90.0, 0.0), [0.0; 10]);
    // Halfway, the left half is hidden.
    assert_eq!(
        wiped(0.5, 90.0, 0.0),
        [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0]
    );
    // At 270 the right half.
    assert_eq!(
        wiped(0.5, 270.0, 0.0),
        [1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn a_feathered_wipe_takes_its_feather_into_its_travel() {
    // With a 40 px fade the edge travels 140 px: at 0 all shows, at 1
    // nothing does, and halfway the fade spans 30 to 70, so pixel 45,
    // centred at 45.5, shows 15.5 / 40 of the layer.
    assert_eq!(wiped(0.0, 90.0, 40.0), [1.0; 10]);
    assert_eq!(wiped(1.0, 90.0, 40.0), [0.0; 10]);
    let half = wiped(0.5, 90.0, 40.0);
    assert_eq!(half[..3], [0.0; 3]);
    assert_eq!(half[7..], [1.0; 3]);
    assert!((half[4] - 0.3875).abs() < 1e-5, "{half:?}");
}

/// A 21 by 21 grey layer with an iris about its middle pixel.
fn iris(values: &[ParamValue]) -> Pixels {
    let mut pixels = solid(21, 21);
    iris_wipe(&mut pixels, values, &ONE);
    pixels
}

fn iris_values(points: f32, outer: f32, inner: f32, feather: f32, star: u8) -> Vec<ParamValue> {
    vec![
        At([10.5, 10.5]),
        Number(points),
        Number(outer),
        Number(inner),
        Number(0.0),
        Number(feather),
        Choice(star),
    ]
}

#[test]
fn an_iris_cuts_a_polygon_hole() {
    let hole = iris(&iris_values(4.0, 8.0, 0.0, 0.0, 0));
    // A square turned 45°: its corners up, right, down and left.
    assert_eq!(hole.get(10, 10)[3], 0.0);
    assert_eq!(hole.get(10, 4)[3], 0.0);
    assert_eq!(hole.get(4, 4)[3], 1.0);
    assert_eq!(hole.get(0, 0)[3], 1.0);
    // Nothing at a radius of 0.
    assert!(
        iris(&iris_values(6.0, 0.0, 0.0, 5.0, 0))
            .data()
            .iter()
            .all(|p| p[3] == 1.0)
    );
}

#[test]
fn an_iris_with_an_inner_radius_is_a_star() {
    let star = iris(&iris_values(8.0, 9.0, 2.0, 0.0, 1));
    // Its tip straight up is cut, the gap between tips is not.
    assert_eq!(star.get(10, 6)[3], 0.0);
    assert_eq!(star.get(13, 7)[3], 1.0);
    let polygon = iris(&iris_values(8.0, 9.0, 2.0, 0.0, 0));
    assert_eq!(polygon.get(13, 7)[3], 0.0);
}

#[test]
fn an_iris_fades_inwards_over_its_feather() {
    let soft = iris(&iris_values(32.0, 8.0, 0.0, 4.0, 0));
    // Hidden within 4 px of the middle, shown from 8 px out.
    assert_eq!(soft.get(10, 10)[3], 0.0);
    let edge = soft.get(16, 10)[3];
    assert!(edge > 0.0 && edge < 1.0, "{edge}");
    assert_eq!(soft.get(20, 10)[3], 1.0);
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

/// Inverting comes before the brightness, which then moves the inverted
/// level: with brightness `b`, a level `v` reads `v + b` plain and
/// `1 - v + b` inverted, so the two add up to `1 + 2b` wherever neither is
/// clipped (inverting after would give 1).
#[test]
fn inverted_noise_is_turned_over_before_its_contrast_and_brightness() {
    let levels = |invert: u8| {
        let mut values = noise_values(0.0, 0.0);
        values[1] = Number(-0.1);
        values[18] = Choice(invert);
        let mut pixels = solid(64, 64);
        fractal_noise(&mut pixels, &values, &ONE);
        pixels.data().iter().map(|p| p[0]).collect::<Vec<f32>>()
    };
    let (plain, inverted) = (levels(0), levels(1));
    let inside = |v: f32| v > 0.001 && v < 0.999;
    let mut checked = 0;
    for (p, i) in plain.iter().zip(&inverted) {
        if inside(*p) && inside(*i) {
            assert!((p + i - 0.8).abs() < 1e-4, "{p} + {i}");
            checked += 1;
        }
    }
    assert!(checked > 1000, "{checked}");
}

/// A white beam along the middle row of a 20 by 5 layer, from x 0 to 20,
/// `length` and `time` given, 3 px thick, its edges hard.
fn beam_at(length: f32, time: f32, composite: u8) -> Pixels {
    let values = [
        At([0.0, 2.5]),
        At([20.0, 2.5]),
        Number(length),
        Number(time),
        Number(3.0),
        Number(3.0),
        Number(0.0),
        color(1.0, 1.0, 1.0),
        color(1.0, 1.0, 1.0),
        Choice(composite),
    ];
    let mut pixels = solid(20, 5);
    beam(&mut pixels, &values, &ONE);
    pixels
}

#[test]
fn a_beam_as_long_as_its_line_spans_it_halfway_through_its_time() {
    let row = |pixels: &Pixels| (0..20).map(|x| pixels.get(x, 2)[3]).collect::<Vec<_>>();
    // At 0.5 the front has reached the end and the back has not left the
    // start.
    assert!(row(&beam_at(1.0, 0.5, 0)).iter().all(|a| *a > 0.99));
    // At 0.25 the front is halfway; at 0.75 the back is.
    let quarter = beam_at(1.0, 0.25, 0);
    close(quarter.get(4, 2), [1.0; 4]);
    close(quarter.get(15, 2), [0.0; 4]);
    let three = beam_at(1.0, 0.75, 0);
    close(three.get(4, 2), [0.0; 4]);
    close(three.get(15, 2), [1.0; 4]);
    // At 0 and 1 it is gone, and in place of the layer nothing is left.
    for time in [0.0, 1.0] {
        assert!(row(&beam_at(1.0, time, 0)).iter().all(|a| *a == 0.0));
    }
}

#[test]
fn a_shorter_beam_runs_from_start_past_end() {
    // Half the line long: at 0.5 its front is three quarters along and its
    // back a quarter.
    let half = beam_at(0.5, 0.5, 0);
    close(half.get(2, 2), [0.0; 4]);
    close(half.get(10, 2), [1.0; 4]);
    close(half.get(17, 2), [0.0; 4]);
}

#[test]
fn a_beam_draws_in_place_of_its_layer_unless_it_composites() {
    // Off the beam, the grey layer is gone, or kept with `composite` on.
    close(beam_at(1.0, 0.5, 0).get(10, 0), [0.0; 4]);
    close(beam_at(1.0, 0.5, 1).get(10, 0), [0.5, 0.5, 0.5, 1.0]);
}

/// The index of `name` in `modes`, as a choice.
fn mode_of(modes: &[&str], name: &str) -> ParamValue {
    Choice(u8::try_from(modes.iter().position(|m| *m == name).unwrap()).unwrap())
}

#[test]
fn every_generator_mode_is_an_operation() {
    assert_eq!(GENERATOR_MODES[0], "replace");
    assert_eq!(NOISE_MODES[0], "over");
    for name in GENERATOR_MODES.iter().chain(NOISE_MODES) {
        assert!(Operation::named(name).is_ok(), "{name}");
    }
    let mut names = GENERATOR_MODES.to_vec();
    names.sort_unstable();
    let mut noise = NOISE_MODES.to_vec();
    noise.sort_unstable();
    assert_eq!(names, noise);
}

#[test]
fn four_colors_blend_with_the_layer_by_their_mode() {
    // Four whites over grey: multiply keeps the grey, screen whitens it,
    // difference halves it, over at half opacity goes half way.
    let blended = |name: &str, opacity: f32| {
        let mut values = vec![];
        for p in [[0.5, 0.5], [20.5, 0.5], [0.5, 20.5], [20.5, 20.5]] {
            values.extend([At(p), color(1.0, 1.0, 1.0)]);
        }
        values.extend([
            Number(100.0),
            Number(opacity),
            mode_of(GENERATOR_MODES, name),
        ]);
        let mut pixels = solid(21, 21);
        four_color(&mut pixels, &values, &ONE);
        pixels.get(10, 10)
    };
    close(blended("multiply", 1.0), [0.5, 0.5, 0.5, 1.0]);
    close(blended("screen", 1.0), [1.0; 4]);
    close(blended("difference", 1.0), [0.5, 0.5, 0.5, 1.0]);
    close(blended("over", 0.5), [0.75, 0.75, 0.75, 1.0]);
    close(blended("screen", 0.5), [0.75, 0.75, 0.75, 1.0]);
    close(blended("replace", 0.5), [0.5, 0.5, 0.5, 0.5]);
}

#[test]
fn a_grid_as_a_stencil_keeps_the_layer_on_its_lines() {
    let values = [
        At([0.0, 0.0]),
        At([0.0, 0.0]),
        Number(10.0),
        Number(10.0),
        Number(2.0),
        color(1.0, 0.0, 0.0),
        Number(1.0),
        Choice(1),
        Choice(0),
        mode_of(GENERATOR_MODES, "stencil-alpha"),
    ];
    let mut pixels = solid(20, 20);
    grid(&mut pixels, &values, &ONE);
    close(pixels.get(10, 5), [0.5, 0.5, 0.5, 1.0]);
    close(pixels.get(5, 5), [0.0; 4]);
}

#[test]
fn fractal_noise_screens_and_multiplies_the_layer() {
    let by = |name: &str| {
        let mut values = noise_values(0.0, 0.0);
        values[19] = mode_of(NOISE_MODES, name);
        let mut pixels = solid(32, 32);
        fractal_noise(&mut pixels, &values, &ONE);
        pixels
    };
    let noise = by("replace");
    let screened = by("screen");
    let multiplied = by("multiply");
    for ((n, s), m) in noise
        .data()
        .iter()
        .zip(screened.data())
        .zip(multiplied.data())
    {
        let v = n[0];
        close(
            *s,
            [0.5 + v - 0.5 * v, 0.5 + v - 0.5 * v, 0.5 + v - 0.5 * v, 1.0],
        );
        close(*m, [0.5 * v, 0.5 * v, 0.5 * v, 1.0]);
    }
}

#[rstest::rstest]
#[case::smeary(4)]
#[case::swirly(5)]
fn warping_fractal_types_move_each_octave_by_the_ones_before(#[case] fractal: u8) {
    let render = |fractal: u8, complexity: f32| {
        let mut values = noise_values(0.0, 0.0);
        values[7] = Number(complexity);
        values[15] = Choice(fractal);
        let mut pixels = solid(64, 64);
        fractal_noise(&mut pixels, &values, &ONE);
        pixels
    };
    let warped = render(fractal, 6.0);
    assert!(warped.data().iter().all(|p| (0.0..=1.0).contains(&p[0])));
    assert_ne!(warped, render(0, 6.0));
    // One octave has none before it to move it: it is the basic noise.
    assert_eq!(render(fractal, 1.0), render(0, 1.0));
}
