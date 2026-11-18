//! The generators: fractal noise by every type, noise, overflow and
//! operation, at the ends of its values, in upright, turned and far-off
//! layers and on a 4K grid; the ramp, four colours, the grid, the blinds,
//! the wipes and the beam by every choice, in the same layers.

use super::*;

/// Parameter values by name.
type Values<'a> = &'a [(&'a str, ParamValue)];

/// The noise's menu of operations (`NOISE_MODES`): 19 rows.
const NOISE_MODES: u8 = 19;

/// The menu of operations of four colours and the grid
/// (`GENERATOR_MODES`): 19 rows.
const GENERATOR_MODES: u8 = 19;

/// `pixels` with a band across them transparent, where a generator must
/// leave them be.
fn holed(mut pixels: Pixels) -> Pixels {
    let width = usize::from(pixels.width());
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        if (i / width) % 17 < 3 {
            *pixel = [0.0; 4];
        }
    }
    pixels
}

fn noise(values: &[(&str, ParamValue)]) -> Effect {
    Effect::new("fractal-noise", values).expect("a fractal noise")
}

/// The values every noise case starts from, so each shows octaves,
/// evolution, a seed and a sub-offset.
fn noise_values(more: &[(&'static str, ParamValue)]) -> Vec<(&'static str, ParamValue)> {
    let mut values = vec![
        ("sub-offset", ParamValue::Point([13.0, -7.0])),
        ("evolution", ParamValue::Number(77.0)),
        ("seed", ParamValue::Number(5.0)),
        ("complexity", ParamValue::Number(6.6)),
        ("contrast", ParamValue::Number(1.6)),
        ("brightness", ParamValue::Number(0.1)),
    ];
    for (name, value) in more {
        values.retain(|(n, _)| n != name);
        values.push((name, *value));
    }
    values
}

/// A layer the transform keeps upright, one it turns, and one far off,
/// shrunk one way and stretched the other.
fn noise_spaces() -> [(&'static str, Space); 3] {
    [
        (
            "upright",
            space(Affine::translate((3.0, -2.0)) * Affine::scale(1.1)),
        ),
        (
            "turned",
            space(Affine::rotate_about(0.4, Point::new(60.0, 40.0)) * Affine::scale(0.8)),
        ),
        (
            "far",
            space(Affine::translate((-9000.0, 7000.0)) * Affine::scale_non_uniform(0.37, 2.9)),
        ),
    ]
}

/// The tolerance row of noise in the space named `space`: a layer far
/// off meets the noise at points some 24 000 px from its origin.
fn noise_row(space: &str) -> &'static str {
    match space {
        "far" => "fractal-noise-far",
        _ => "fractal-noise",
    }
}

#[test]
fn fractal_noise_matches_by_every_type_noise_and_overflow() {
    let pixels = holed(pattern(197, 131, 50));
    for (name, space) in noise_spaces() {
        for fractal in 0..6u8 {
            for kind in 0..4u8 {
                for overflow in 0..3u8 {
                    let effect = noise(&noise_values(&[
                        ("fractal", ParamValue::Choice(fractal)),
                        ("noise", ParamValue::Choice(kind)),
                        ("overflow", ParamValue::Choice(overflow)),
                        ("polarity", ParamValue::Choice((fractal + overflow) % 2)),
                    ]));
                    if kind == 3 && fractal == 5 {
                        if let Some(gpu) = gpu() {
                            assert!(!gpu.runs(&effect), "swirly block noise stays on the CPU");
                        }
                        continue;
                    }
                    effect_matches(
                        noise_row(name),
                        &format!("{name} fractal {fractal} noise {kind} overflow {overflow}"),
                        &effect,
                        &pixels,
                        &space,
                    );
                }
            }
        }
    }
}

#[test]
fn turned_noise_matches() {
    let pixels = holed(pattern(149, 103, 51));
    for (name, space) in noise_spaces() {
        for fractal in [0u8, 2, 4, 5] {
            for kind in 0..4u8 {
                for (rotation, sub_rotation) in [(33.0f32, 0.0f32), (0.0, -71.0), (190.0, 12.0)] {
                    let effect = noise(&noise_values(&[
                        ("fractal", ParamValue::Choice(fractal)),
                        ("noise", ParamValue::Choice(kind)),
                        ("rotation", ParamValue::Number(rotation)),
                        ("sub-rotation", ParamValue::Number(sub_rotation)),
                    ]));
                    if kind == 3 && fractal == 5 {
                        continue;
                    }
                    effect_matches(
                        noise_row(name),
                        &format!(
                            "{name} fractal {fractal} noise {kind} turned {rotation} {sub_rotation}"
                        ),
                        &effect,
                        &pixels,
                        &space,
                    );
                }
            }
        }
    }
}

#[test]
fn fractal_noise_combines_by_every_operation() {
    let pixels = holed(pattern(131, 97, 52));
    let (_, space) = &noise_spaces()[1];
    for mode in 0..NOISE_MODES {
        for opacity in [1.0f32, 0.45] {
            let effect = noise(&noise_values(&[
                ("mode", ParamValue::Choice(mode)),
                ("opacity", ParamValue::Number(opacity)),
            ]));
            effect_matches(
                "fractal-noise",
                &format!("mode {mode} opacity {opacity}"),
                &effect,
                &pixels,
                space,
            );
        }
    }
}

#[test]
fn fractal_noise_matches_at_the_ends_of_its_values() {
    let pixels = holed(pattern(157, 89, 53));
    let ends: &[(&str, Values<'_>)] = &[
        ("one octave", &[("complexity", ParamValue::Number(1.0))]),
        (
            "twenty octaves",
            &[("complexity", ParamValue::Number(20.0))],
        ),
        (
            "no influence",
            &[("sub-influence", ParamValue::Number(0.0))],
        ),
        (
            "most influence",
            &[
                ("sub-influence", ParamValue::Number(100.0)),
                ("complexity", ParamValue::Number(20.0)),
            ],
        ),
        (
            "finest scaling",
            &[
                ("sub-scaling", ParamValue::Number(0.1)),
                ("complexity", ParamValue::Number(20.0)),
            ],
        ),
        (
            "coarsest scaling",
            &[("sub-scaling", ParamValue::Number(100.0))],
        ),
        (
            "smallest",
            &[
                ("scale", ParamValue::Number(0.01)),
                ("scale-width", ParamValue::Number(0.01)),
            ],
        ),
        (
            "largest",
            &[
                ("scale", ParamValue::Number(100.0)),
                ("scale-height", ParamValue::Number(100.0)),
            ],
        ),
        ("stretched", &[("scale-width", ParamValue::Number(7.0))]),
        (
            "far offset",
            &[("offset", ParamValue::Point([-25_000.0, 18_000.0]))],
        ),
        ("last seed", &[("seed", ParamValue::Number(100_000.0))]),
        (
            "far evolution",
            &[("evolution", ParamValue::Number(-123_456.0))],
        ),
        ("no contrast", &[("contrast", ParamValue::Number(0.0))]),
        ("brightest", &[("brightness", ParamValue::Number(100.0))]),
        ("darkest", &[("brightness", ParamValue::Number(-100.0))]),
    ];
    for (case, values) in ends {
        for kind in [0u8, 3] {
            for overflow in 0..3u8 {
                let mut more = vec![
                    ("noise", ParamValue::Choice(kind)),
                    ("overflow", ParamValue::Choice(overflow)),
                ];
                more.extend_from_slice(values);
                let effect = noise(&noise_values(&more));
                let row = match *case {
                    "far offset" => "fractal-noise-far",
                    _ => "fractal-noise",
                };
                effect_matches(
                    row,
                    &format!("{case} noise {kind} overflow {overflow}"),
                    &effect,
                    &pixels,
                    &noise_spaces()[0].1,
                );
            }
        }
    }
}

#[test]
fn contrasted_noise_matches_within_its_contrast() {
    // Contrast multiplies every level's difference from mid grey, and
    // with it the GPU's rounding differences.
    let pixels = holed(pattern(143, 91, 54));
    for (name, space) in noise_spaces() {
        for kind in 0..4u8 {
            for overflow in 0..3u8 {
                let effect = noise(&noise_values(&[
                    ("noise", ParamValue::Choice(kind)),
                    ("overflow", ParamValue::Choice(overflow)),
                    ("contrast", ParamValue::Number(100.0)),
                    ("rotation", ParamValue::Number(21.0)),
                ]));
                let row = noise_row(name);
                effect_matches(
                    row,
                    &format!("{name} noise {kind} overflow {overflow}"),
                    &effect,
                    &pixels,
                    &space,
                );
            }
        }
    }
}

#[test]
fn fractal_noise_matches_on_a_4k_grid() {
    let pixels = pattern(3840, 2160, 55);
    let mut frame = space(Affine::IDENTITY);
    frame.bounds = None;
    let turned = space(Affine::rotate_about(0.4, Point::new(1900.0, 1000.0)));
    let far = space(
        Affine::rotate_about(0.4, Point::new(1900.0, 1000.0))
            * Affine::translate((-30_000.0, 20_000.0))
            * Affine::scale(0.3),
    );
    let cases: &[(&str, &Space, Values<'_>)] = &[
        ("spline", &frame, &[]),
        ("block", &frame, &[("noise", ParamValue::Choice(3))]),
        (
            "turned spline",
            &frame,
            &[("rotation", ParamValue::Number(33.0))],
        ),
        ("swirly", &frame, &[("fractal", ParamValue::Choice(5))]),
        ("spline in a turned layer", &turned, &[]),
        (
            "block in a turned layer",
            &turned,
            &[("noise", ParamValue::Choice(3))],
        ),
        (
            "block in a far, turned layer",
            &far,
            &[
                ("noise", ParamValue::Choice(3)),
                ("complexity", ParamValue::Number(9.0)),
            ],
        ),
    ];
    for (case, space, values) in cases {
        let row = match *case {
            "block in a far, turned layer" => "fractal-noise-far",
            _ => "fractal-noise",
        };
        effect_matches(
            row,
            &format!("4K {case}"),
            &noise(&noise_values(values)),
            &pixels,
            space,
        );
    }
}

#[test]
fn fractal_noise_matches_on_the_smallest_grids() {
    for (width, height) in [(1u16, 1u16), (3, 1), (1, 5)] {
        let pixels = pattern(width, height, 56);
        effect_matches(
            "fractal-noise",
            &format!("{width} by {height}"),
            &noise(&noise_values(&[])),
            &pixels,
            &noise_spaces()[1].1,
        );
    }
}

fn generator(name: &str, values: &[(&str, ParamValue)]) -> Effect {
    Effect::new(name, values).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn colour(r: f32, g: f32, b: f32, a: f32) -> ParamValue {
    ParamValue::Color(nakkash_raster::Color::rgba(r, g, b, a))
}

/// Runs `effect` in every space of [`noise_spaces`] and a layer without a
/// box, checking it against `name`'s row.
fn matches_everywhere(name: &str, case: &str, effect: &Effect, pixels: &Pixels) {
    let mut unboxed = space(Affine::translate((-20.0, 15.0)) * Affine::scale(1.3));
    unboxed.bounds = None;
    for (space_name, space) in noise_spaces().into_iter().chain([("unboxed", unboxed)]) {
        effect_matches(
            name,
            &format!("{case} {space_name}"),
            effect,
            pixels,
            &space,
        );
    }
}

#[test]
fn ramps_match_by_shape_and_blend() {
    let pixels = holed(pattern(161, 107, 60));
    for shape in 0..2u8 {
        for blend in [0.0f32, 0.4, 1.0] {
            for end in [[150.0f32, 90.0], [12.0, 30.0]] {
                let effect = generator(
                    "ramp",
                    &[
                        ("start", ParamValue::Point([12.0, 30.0])),
                        ("start-color", colour(0.1, 0.9, 0.2, 0.7)),
                        ("end", ParamValue::Point(end)),
                        ("end-color", colour(0.8, 0.3, 1.0, 0.2)),
                        ("blend", ParamValue::Number(blend)),
                        ("shape", ParamValue::Choice(shape)),
                    ],
                );
                matches_everywhere(
                    "ramp",
                    &format!("shape {shape} blend {blend} end {end:?}"),
                    &effect,
                    &pixels,
                );
            }
        }
    }
}

#[test]
fn four_colors_match_by_every_operation_and_blend() {
    let pixels = holed(pattern(149, 101, 61));
    for mode in 0..GENERATOR_MODES {
        let effect = generator(
            "four-color",
            &[
                ("point-1", ParamValue::Point([10.5, 20.5])),
                ("point-3", ParamValue::Point([30.0, 140.0])),
                ("blend", ParamValue::Number(37.0)),
                ("opacity", ParamValue::Number(0.8)),
                ("mode", ParamValue::Choice(mode)),
            ],
        );
        matches_everywhere("four-color", &format!("mode {mode}"), &effect, &pixels);
    }
    for blend in [1.0f32, 10.0, 100.0, 750.0, 10_000.0] {
        let effect = generator(
            "four-color",
            &[
                ("point-2", ParamValue::Point([120.0, 10.0])),
                ("blend", ParamValue::Number(blend)),
                ("mode", ParamValue::Choice(1)),
            ],
        );
        matches_everywhere("four-color", &format!("blend {blend}"), &effect, &pixels);
    }
}

#[test]
fn grids_match_by_size_draw_and_operation() {
    let pixels = holed(pattern(171, 113, 62));
    for size in 0..3u8 {
        for draw in 0..2u8 {
            for border in [0.0f32, 3.3, 40.0] {
                let effect = generator(
                    "grid",
                    &[
                        ("anchor", ParamValue::Point([7.25, -3.0])),
                        ("corner", ParamValue::Point([30.0, 21.5])),
                        ("width", ParamValue::Number(17.0)),
                        ("height", ParamValue::Number(9.5)),
                        ("border", ParamValue::Number(border)),
                        ("opacity", ParamValue::Number(0.9)),
                        ("size", ParamValue::Choice(size)),
                        ("draw", ParamValue::Choice(draw)),
                        ("mode", ParamValue::Choice(1)),
                    ],
                );
                matches_everywhere(
                    "grid",
                    &format!("size {size} draw {draw} border {border}"),
                    &effect,
                    &pixels,
                );
            }
        }
    }
    for mode in 0..GENERATOR_MODES {
        let effect = generator(
            "grid",
            &[
                ("border", ParamValue::Number(4.0)),
                ("color", colour(0.9, 0.4, 0.1, 1.0)),
                ("mode", ParamValue::Choice(mode)),
            ],
        );
        effect_matches(
            "grid",
            &format!("mode {mode}"),
            &effect,
            &pixels,
            &noise_spaces()[1].1,
        );
    }
}

#[test]
fn venetian_blinds_match_with_hard_and_soft_edges() {
    let pixels = holed(pattern(163, 109, 63));
    for completion in [0.0f32, 0.37, 1.0] {
        for direction in [0.0f32, 33.0, 90.0, -170.0] {
            for (width, feather) in [(20.0f32, 0.0f32), (1.0, 0.0), (333.0, 4.5), (20.0, 1000.0)] {
                let effect = generator(
                    "venetian-blinds",
                    &[
                        ("completion", ParamValue::Number(completion)),
                        ("direction", ParamValue::Number(direction)),
                        ("width", ParamValue::Number(width)),
                        ("feather", ParamValue::Number(feather)),
                    ],
                );
                matches_everywhere(
                    "venetian-blinds",
                    &format!("{completion} {direction}° width {width} feather {feather}"),
                    &effect,
                    &pixels,
                );
            }
        }
    }
}

#[test]
fn linear_wipes_match_at_every_angle_and_feather() {
    let pixels = holed(pattern(157, 103, 64));
    for completion in [0.0f32, 0.3, 0.77, 1.0] {
        for angle in [0.0f32, 90.0, 123.0, 300.0] {
            for feather in [0.0f32, 12.0, 5000.0] {
                let effect = generator(
                    "linear-wipe",
                    &[
                        ("completion", ParamValue::Number(completion)),
                        ("angle", ParamValue::Number(angle)),
                        ("feather", ParamValue::Number(feather)),
                    ],
                );
                matches_everywhere(
                    "linear-wipe",
                    &format!("{completion} {angle}° feather {feather}"),
                    &effect,
                    &pixels,
                );
            }
        }
    }
}

#[test]
fn iris_wipes_match_as_polygons_and_stars() {
    let pixels = holed(pattern(167, 111, 65));
    for points in [3.0f32, 7.4, 32.0] {
        for inner in 0..2u8 {
            for (outer, radius) in [(60.0f32, 25.0f32), (0.0, 30.0), (900.0, 450.0)] {
                for feather in [0.0f32, 9.0] {
                    let effect = generator(
                        "iris-wipe",
                        &[
                            ("center", ParamValue::Point([90.0, 70.0])),
                            ("points", ParamValue::Number(points)),
                            ("outer-radius", ParamValue::Number(outer)),
                            ("inner-radius", ParamValue::Number(radius)),
                            ("rotation", ParamValue::Number(17.0)),
                            ("feather", ParamValue::Number(feather)),
                            ("inner", ParamValue::Choice(inner)),
                        ],
                    );
                    matches_everywhere(
                        "iris-wipe",
                        &format!(
                            "{points} points inner {inner} {outer}/{radius} feather {feather}"
                        ),
                        &effect,
                        &pixels,
                    );
                }
            }
        }
    }
}

#[test]
fn beams_match_along_their_time_and_thickness() {
    let pixels = holed(pattern(173, 119, 66));
    for composite in 0..2u8 {
        for (length, time) in [
            (0.4f32, 0.0f32),
            (0.4, 0.3),
            (1.0, 0.5),
            (0.0, 0.6),
            (0.4, 1.0),
        ] {
            for (from, to, softness) in [
                (3.0f32, 17.0f32, 0.5f32),
                (0.0, 0.0, 0.0),
                (40.0, 4000.0, 1.0),
            ] {
                for end in [[170.0f32, 110.0], [5.0, 10.0]] {
                    let effect = generator(
                        "beam",
                        &[
                            ("start", ParamValue::Point([5.0, 10.0])),
                            ("end", ParamValue::Point(end)),
                            ("length", ParamValue::Number(length)),
                            ("time", ParamValue::Number(time)),
                            ("start-thickness", ParamValue::Number(from)),
                            ("end-thickness", ParamValue::Number(to)),
                            ("softness", ParamValue::Number(softness)),
                            ("inside-color", colour(1.0, 0.75, 0.98, 0.9)),
                            ("outside-color", colour(0.1, 0.2, 0.9, 0.4)),
                            ("composite", ParamValue::Choice(composite)),
                        ],
                    );
                    matches_everywhere(
                        "beam",
                        &format!(
                            "composite {composite} {length}/{time} {from}..{to} soft {softness} to {end:?}"
                        ),
                        &effect,
                        &pixels,
                    );
                }
            }
        }
    }
}

#[test]
fn a_beam_stops_at_the_edges_of_its_layer_s_box() {
    let pixels = holed(pattern(151, 97, 68));
    let mut upright = space(Affine::translate((4.0, -3.0)) * Affine::scale(1.3));
    upright.bounds = Some(nakkash_raster::kurbo::Rect::new(20.0, 15.0, 90.5, 61.25));
    let mut turned = space(Affine::rotate_about(-0.6, Point::new(70.0, 50.0)));
    turned.bounds = upright.bounds;
    for composite in 0..2u8 {
        let effect = generator(
            "beam",
            &[
                ("start", ParamValue::Point([0.0, 5.0])),
                ("end", ParamValue::Point([140.0, 80.0])),
                ("length", ParamValue::Number(1.0)),
                ("start-thickness", ParamValue::Number(30.0)),
                ("end-thickness", ParamValue::Number(55.0)),
                ("composite", ParamValue::Choice(composite)),
            ],
        );
        for (name, space) in [("upright", &upright), ("turned", &turned)] {
            effect_matches(
                "beam",
                &format!("boxed {name} composite {composite}"),
                &effect,
                &pixels,
                space,
            );
        }
    }
}

#[test]
fn generators_match_on_a_4k_grid() {
    let pixels = pattern(3840, 2160, 67);
    let mut frame = space(Affine::IDENTITY);
    frame.bounds = Some(nakkash_raster::kurbo::Rect::new(0.0, 0.0, 3840.0, 2160.0));
    let turned = space(Affine::rotate_about(0.4, Point::new(1900.0, 1000.0)));
    let cases: Vec<(&str, Effect)> = vec![
        (
            "ramp",
            generator(
                "ramp",
                &[
                    ("start", ParamValue::Point([100.0, 3000.0])),
                    ("end", ParamValue::Point([3700.0, 200.0])),
                ],
            ),
        ),
        (
            "four-color",
            generator(
                "four-color",
                &[
                    ("point-4", ParamValue::Point([3500.0, 2000.0])),
                    ("mode", ParamValue::Choice(1)),
                ],
            ),
        ),
        (
            "grid",
            generator("grid", &[("border", ParamValue::Number(2.5))]),
        ),
        (
            "venetian-blinds",
            generator(
                "venetian-blinds",
                &[
                    ("completion", ParamValue::Number(0.4)),
                    ("direction", ParamValue::Number(21.0)),
                ],
            ),
        ),
        (
            "linear-wipe",
            generator(
                "linear-wipe",
                &[
                    ("completion", ParamValue::Number(0.5)),
                    ("angle", ParamValue::Number(123.0)),
                ],
            ),
        ),
        (
            "iris-wipe",
            generator(
                "iris-wipe",
                &[
                    ("center", ParamValue::Point([1900.0, 1100.0])),
                    ("outer-radius", ParamValue::Number(900.0)),
                ],
            ),
        ),
        (
            "beam",
            generator(
                "beam",
                &[
                    ("start", ParamValue::Point([30.0, 2100.0])),
                    ("end", ParamValue::Point([3800.0, 40.0])),
                    ("length", ParamValue::Number(1.0)),
                    ("start-thickness", ParamValue::Number(3.0)),
                    ("end-thickness", ParamValue::Number(60.0)),
                ],
            ),
        ),
    ];
    for (name, effect) in &cases {
        for (space_name, space) in [("upright", &frame), ("turned", &turned)] {
            effect_matches(name, &format!("4K {space_name}"), effect, &pixels, space);
        }
    }
}
