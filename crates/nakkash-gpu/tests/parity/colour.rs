//! The colour effects, the fill, the sharpen and the edge finder: every
//! choice of their parameters, odd grids, transparent areas, colours with
//! ties between their channels, and layers with and without a box.

use nakkash_raster::kurbo::{Affine, Point, Rect};
use nakkash_raster::{Accelerator, Color, Curve, Effect, ParamValue, Pixels, Space};

use super::{Random, effect_matches, gpu, pattern, space};

/// [`pattern`], with columns that test the colour effects' branches: greys,
/// colours with two equal channels, pure and mixed primaries, random
/// colours, and alphas from barely there to opaque.
fn colours(width: u16, height: u16, seed: u64) -> Pixels {
    let base = pattern(width, height, seed);
    let mut random = Random(seed ^ 0x5eed);
    let w = usize::from(width);
    let data = base
        .data()
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let x = i % w;
            let a = match x % 9 {
                0 => 1.0,
                1 => 1e-3,
                _ => p[3],
            };
            let v = random.next();
            let rgb = match x % 13 {
                0 => [v; 3],
                1 => [v, v, random.next()],
                2 => [random.next(), v, v],
                3 => [v, random.next(), v],
                4 => [1.0, 0.0, 0.0],
                5 => [1.0, 1.0, 0.0],
                6 => [0.0, 1.0, 1.0],
                7 => [0.0, 0.0, 1.0],
                8 => [random.next(), random.next(), random.next()],
                _ => match p[3] > 0.0 {
                    true => [p[0] / p[3], p[1] / p[3], p[2] / p[3]],
                    false => [0.0; 3],
                },
            };
            match p[3] > 0.0 || x % 9 < 2 {
                true => [rgb[0] * a, rgb[1] * a, rgb[2] * a, a],
                false => [0.0; 4],
            }
        })
        .collect();
    Pixels::from_data(width, height, data)
}

/// The spaces a layer's pixels are in: upright and scaled, turned, and
/// without a box.
fn spaces() -> Vec<(&'static str, Space)> {
    let upright = space(Affine::translate((17.5, -9.0)) * Affine::scale_non_uniform(0.45, 0.3));
    let mut turned = space(Affine::rotate_about(0.35, Point::new(90.0, 60.0)) * Affine::scale(0.4));
    turned.bounds = Some(Rect::new(30.0, 25.0, 390.0, 260.0));
    let mut boxless = space(Affine::rotate(0.2) * Affine::scale(1.1));
    boxless.bounds = None;
    vec![
        ("upright", upright),
        ("turned", turned),
        ("boxless", boxless),
    ]
}

/// Runs `name` with `params` on [`colours`] in every one of [`spaces`],
/// checked against the tolerance named `row`.
fn matches(row: &str, name: &str, params: &[(&str, ParamValue)]) {
    let effect = Effect::new(name, params).expect("an effect");
    for (size, seed) in [((181, 117), 50), ((64, 1), 51), ((1, 37), 52)] {
        let pixels = colours(size.0, size.1, seed);
        for (space_name, space) in spaces() {
            let case = format!("{params:?} {size:?} {space_name}");
            effect_matches(row, &case, &effect, &pixels, &space);
        }
    }
}

fn curve(points: &[[f32; 2]]) -> ParamValue {
    ParamValue::Curve(Curve::new(points).expect("a curve"))
}

fn colour(r: f32, g: f32, b: f32, a: f32) -> ParamValue {
    ParamValue::Color(Color::rgba(r, g, b, a))
}

#[test]
fn every_effect_of_the_family_runs_on_the_gpu() {
    let Some(gpu) = gpu() else { return };
    for name in [
        "curves",
        "find-edges",
        "tint",
        "fill",
        "sharpen",
        "invert",
        "levels",
        "hue-saturation",
        "exposure",
        "black-white",
    ] {
        let effect = Effect::new(name, &[]).expect("an effect");
        assert!(gpu.runs(&effect), "{name}");
    }
}

#[test]
fn curves_match_on_every_channel() {
    let shapes = [
        vec![[0.0, 0.1], [0.4, 0.7], [1.0, 0.9]],
        vec![[0.2, 0.0], [0.5, 1.0], [0.8, 0.0]],
        vec![[0.5, 0.5]],
        vec![[0.0, 1.0], [0.3, 0.2], [0.31, 0.9], [1.0, 0.0]],
    ];
    let names = ["rgb", "red", "green", "blue", "alpha"];
    for points in &shapes {
        for name in names {
            matches("curves", "curves", &[(name, curve(points))]);
        }
    }
    matches(
        "curves",
        "curves",
        &[
            ("rgb", curve(&shapes[0])),
            ("red", curve(&shapes[1])),
            ("green", curve(&shapes[3])),
            ("blue", curve(&shapes[2])),
            ("alpha", curve(&[[0.0, 0.2], [1.0, 1.0]])),
        ],
    );
    // Straight lines change nothing, on both.
    matches("curves", "curves", &[]);
}

#[test]
fn tints_match() {
    matches("tint", "tint", &[]);
    matches(
        "tint",
        "tint",
        &[
            ("black", colour(0.2, 0.1, 0.5, 1.0)),
            ("white", colour(1.0, 0.8, 0.3, 0.4)),
            ("amount", ParamValue::Number(0.6)),
        ],
    );
}

#[test]
fn hues_and_saturations_match_in_both_modes() {
    for (hue, saturation, lightness) in [
        (40.0, 0.3, -0.2),
        (-130.0, -0.6, 0.4),
        (725.0, 1.0, -1.0),
        (0.0, -1.0, 1.0),
    ] {
        matches(
            "hue-saturation",
            "hue-saturation",
            &[
                ("hue", ParamValue::Number(hue)),
                ("saturation", ParamValue::Number(saturation)),
                ("lightness", ParamValue::Number(lightness)),
            ],
        );
        matches(
            "hue-saturation",
            "hue-saturation",
            &[
                ("colorize-hue", ParamValue::Number(hue)),
                ("colorize-saturation", ParamValue::Number(saturation.abs())),
                ("colorize-lightness", ParamValue::Number(lightness)),
                ("mode", ParamValue::Choice(1)),
            ],
        );
    }
}

#[test]
fn exposures_match_in_both_spaces() {
    for (stops, offset, gamma) in [
        (1.3, 0.05, 1.4),
        (-0.7, -0.1, 0.6),
        (0.0, 0.0, 1.0),
        (2.5, 0.3, 2.2),
        (-6.0, 1.5, 10.0),
        (4.0, -2.0, 0.1),
    ] {
        for space in [0, 1] {
            matches(
                "exposure",
                "exposure",
                &[
                    ("exposure", ParamValue::Number(stops)),
                    ("offset", ParamValue::Number(offset)),
                    ("gamma", ParamValue::Number(gamma)),
                    ("space", ParamValue::Choice(space)),
                ],
            );
        }
    }
}

#[test]
fn levels_match_on_every_channel() {
    for channel in 0..5 {
        for (input_black, input_white, gamma, output_black, output_white) in [
            (0.1, 0.8, 1.6, 0.05, 0.9),
            (0.2, 0.9, 0.0, 0.3, 0.7),
            (0.9, 0.1, 0.5, 1.0, 0.0),
            (-0.5, 1.5, 4.0, -0.2, 1.3),
        ] {
            matches(
                "levels",
                "levels",
                &[
                    ("input-black", ParamValue::Number(input_black)),
                    ("input-white", ParamValue::Number(input_white)),
                    ("gamma", ParamValue::Number(gamma)),
                    ("output-black", ParamValue::Number(output_black)),
                    ("output-white", ParamValue::Number(output_white)),
                    ("channel", ParamValue::Choice(channel)),
                ],
            );
        }
        // An input range of no width: a threshold at its white.
        matches(
            "levels-threshold",
            "levels",
            &[
                ("input-black", ParamValue::Number(0.5)),
                ("input-white", ParamValue::Number(0.5)),
                ("channel", ParamValue::Choice(channel)),
            ],
        );
    }
}

#[test]
fn inverts_match_on_every_channel() {
    for channel in 0..5 {
        for blend in [0.0, 0.3, 1.0] {
            matches(
                "invert",
                "invert",
                &[
                    ("blend", ParamValue::Number(blend)),
                    ("channel", ParamValue::Choice(channel)),
                ],
            );
        }
    }
}

#[test]
fn black_and_whites_match() {
    matches("black-white", "black-white", &[]);
    matches(
        "black-white",
        "black-white",
        &[
            ("reds", ParamValue::Number(-0.5)),
            ("yellows", ParamValue::Number(3.0)),
            ("greens", ParamValue::Number(-2.0)),
            ("cyans", ParamValue::Number(2.0)),
            ("blues", ParamValue::Number(0.9)),
            ("magentas", ParamValue::Number(0.1)),
            ("tint", ParamValue::Number(0.6)),
            ("tint-color", colour(0.2, 0.5, 0.9, 1.0)),
        ],
    );
}

#[test]
fn fills_match_inverted_or_not_in_and_out_of_a_box() {
    for inverted in [0, 1] {
        for (color, opacity) in [
            (colour(0.3, 0.6, 0.9, 1.0), 1.0),
            (colour(0.9, 0.2, 0.1, 0.5), 0.7),
        ] {
            matches(
                "fill",
                "fill",
                &[
                    ("color", color),
                    ("opacity", ParamValue::Number(opacity)),
                    ("inverted", ParamValue::Choice(inverted)),
                ],
            );
        }
    }
}

#[test]
fn a_box_s_edge_through_pixel_centres_is_where_the_cpu_puts_it() {
    // Box edges at exact pixel centres, upright and turned by a right
    // angle: a centre on an edge is in the box at its start and out at its
    // end, and a box test worked out in f32 would decide some of them the
    // other way.
    let pixels = colours(97, 61, 53);
    let mut on_centres = space(Affine::IDENTITY);
    on_centres.bounds = Some(Rect::new(10.5, 4.5, 60.5, 40.5));
    let mut quarter = space(Affine::rotate_about(
        std::f64::consts::FRAC_PI_2,
        Point::new(48.0, 30.0),
    ));
    quarter.bounds = Some(Rect::new(20.5, 7.5, 70.5, 50.5));
    let fill = Effect::new(
        "fill",
        &[
            ("color", colour(0.3, 0.6, 0.9, 1.0)),
            ("inverted", ParamValue::Choice(1)),
        ],
    )
    .expect("a fill");
    for (name, space) in [("on centres", &on_centres), ("a quarter turn", &quarter)] {
        effect_matches("fill", name, &fill, &pixels, space);
    }
}

#[test]
fn sharpens_match() {
    for amount in [0.0, 50.0, 400.0, 4000.0] {
        matches(
            "sharpen",
            "sharpen",
            &[("amount", ParamValue::Number(amount))],
        );
    }
}

#[test]
fn edges_match() {
    for blend in [0.0, 0.35, 1.0] {
        for lines in [0, 1] {
            matches(
                "find-edges",
                "find-edges",
                &[
                    ("blend", ParamValue::Number(blend)),
                    ("lines", ParamValue::Choice(lines)),
                ],
            );
        }
    }
}
