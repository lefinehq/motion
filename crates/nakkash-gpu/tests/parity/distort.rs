//! The distortions: every choice of each, in spaces that scale, turn and
//! move the layer, with and without a box of its own, on odd grids.

use nakkash_raster::kurbo::{Affine, Point, Rect};
use nakkash_raster::{Accelerator, Effect, Input, Layer, Node, ParamValue, Pixels, Space};

use super::{effect_matches, gpu, pattern, space};

/// The spaces every distortion runs in: a box larger than the grid and
/// scaled onto it, one turned and moved, and none, the frame's box.
fn spaces() -> Vec<(&'static str, Space)> {
    let scaled = space(Affine::translate((3.25, -2.5)) * Affine::scale_non_uniform(0.47, 0.41));
    let mut turned = space(
        Affine::translate((14.0, -6.0))
            * Affine::rotate_about(0.31, Point::new(90.0, 60.0))
            * Affine::scale(0.43),
    );
    turned.bounds = Some(Rect::new(22.5, 17.25, 371.0, 281.5));
    let mut frame = space(Affine::translate((-7.5, 4.25)) * Affine::scale(1.13));
    frame.bounds = None;
    vec![("scaled", scaled), ("turned", turned), ("frame", frame)]
}

/// Runs `effect` with `values` in every space, as `name`'s parity case.
fn matches(name: &str, tolerance: &str, case: &str, values: &[(&str, ParamValue)]) {
    let effect = Effect::new(name, values).expect("an effect");
    let pixels = pattern(183, 119, 51);
    for (space_name, space) in spaces() {
        effect_matches(
            tolerance,
            &format!("{case} {space_name}"),
            &effect,
            &pixels,
            &space,
        );
    }
}

#[test]
fn motion_tiles_match() {
    for (center, tile, output, phase) in [
        ([200.0, 150.0], [1.0, 1.0], [1.0, 1.0], 0.0),
        ([130.0, 95.0], [0.31, 0.43], [1.0, 1.0], 0.0),
        ([217.0, 160.0], [0.37, 0.29], [1.6, 2.3], 45.0),
        ([40.0, 260.0], [0.53, 0.61], [0.7, 0.55], -100.0),
    ] {
        for mirror in 0..2 {
            for shift in 0..2 {
                matches(
                    "motion-tile",
                    "motion-tile",
                    &format!("{center:?} {tile:?} {output:?} {phase} {mirror} {shift}"),
                    &[
                        ("center", ParamValue::Point(center)),
                        ("tile-width", ParamValue::Number(tile[0])),
                        ("tile-height", ParamValue::Number(tile[1])),
                        ("output-width", ParamValue::Number(output[0])),
                        ("output-height", ParamValue::Number(output[1])),
                        ("phase", ParamValue::Number(phase)),
                        ("edges", ParamValue::Choice(mirror)),
                        ("shift", ParamValue::Choice(shift)),
                    ],
                );
            }
        }
    }
}

#[test]
fn wraps_match() {
    for (to, blend) in [
        ([200.0, 150.0], 0.0),
        ([61.0, 233.0], 0.0),
        ([317.0, 44.0], 0.35),
        ([-410.0, 905.0], 1.0),
    ] {
        matches(
            "wrap",
            "wrap",
            &format!("{to:?} {blend}"),
            &[
                ("to", ParamValue::Point(to)),
                ("blend", ParamValue::Number(blend)),
            ],
        );
    }
}

#[test]
fn mirrors_match() {
    for (center, angle) in [
        ([200.0, 150.0], 0.0),
        ([173.0, 121.0], 37.0),
        ([90.0, 201.0], 90.0),
        ([260.0, 80.0], 213.0),
        ([-50.0, 400.0], -61.0),
    ] {
        matches(
            "mirror",
            "mirror",
            &format!("{center:?} {angle}"),
            &[
                ("center", ParamValue::Point(center)),
                ("angle", ParamValue::Number(angle)),
            ],
        );
    }
}

#[test]
fn polar_coordinates_match() {
    for amount in [0.0, 0.37, 1.0] {
        for conversion in 0..2 {
            matches(
                "polar-coordinates",
                "polar-coordinates",
                &format!("{amount} {conversion}"),
                &[
                    ("interpolation", ParamValue::Number(amount)),
                    ("conversion", ParamValue::Choice(conversion)),
                ],
            );
        }
    }
}

#[test]
fn lens_distortions_match() {
    for (degrees, center) in [
        (0.0, [200.0, 150.0]),
        (63.0, [200.0, 150.0]),
        (121.0, [143.0, 187.0]),
        (179.0, [251.0, 99.0]),
        (180.0, [200.0, 150.0]),
    ] {
        for remove in 0..2 {
            for field in 0..3 {
                for grow in 0..3 {
                    matches(
                        "lens-distortion",
                        "lens-distortion",
                        &format!("{degrees} {center:?} {remove} {field} {grow}"),
                        &[
                            ("field-of-view", ParamValue::Number(degrees)),
                            ("center", ParamValue::Point(center)),
                            ("distortion", ParamValue::Choice(remove)),
                            ("field", ParamValue::Choice(field)),
                            ("grow", ParamValue::Choice(grow)),
                        ],
                    );
                }
            }
        }
    }
}

#[test]
fn repeat_tiles_match() {
    for reach in [
        [0.0; 4],
        [130.0, 45.0, 210.0, 17.0],
        [900.0, 900.0, 900.0, 900.0],
    ] {
        for tiling in 0..4 {
            matches(
                "repeat-tile",
                "repeat-tile",
                &format!("{reach:?} {tiling}"),
                &[
                    ("right", ParamValue::Number(reach[0])),
                    ("left", ParamValue::Number(reach[1])),
                    ("down", ParamValue::Number(reach[2])),
                    ("up", ParamValue::Number(reach[3])),
                    ("tiling", ParamValue::Choice(tiling)),
                ],
            );
        }
    }
}

#[test]
fn turbulent_displacements_match() {
    for displacement in 0..9 {
        for pinning in 0..4 {
            matches(
                "turbulent-displace",
                "turbulent-displace",
                &format!("{displacement} {pinning}"),
                &[
                    ("amount", ParamValue::Number(63.0)),
                    ("size", ParamValue::Number(57.0)),
                    ("offset", ParamValue::Point([13.0, -29.0])),
                    ("complexity", ParamValue::Number(2.6)),
                    ("evolution", ParamValue::Number(77.0)),
                    ("seed", ParamValue::Number(5.0)),
                    ("displacement", ParamValue::Choice(displacement)),
                    ("pinning", ParamValue::Choice(pinning)),
                ],
            );
        }
    }
    // Every octave of the most complex noise, a cell of the least size,
    // and a reach far past the box.
    for (amount, size, complexity) in [(10_000.0, 2.0, 10.0), (-400.0, 1000.0, 1.0)] {
        matches(
            "turbulent-displace",
            "turbulent-displace",
            &format!("{amount} {size} {complexity}"),
            &[
                ("amount", ParamValue::Number(amount)),
                ("size", ParamValue::Number(size)),
                ("complexity", ParamValue::Number(complexity)),
                ("seed", ParamValue::Number(91_000.0)),
            ],
        );
    }
}

/// `displacement-map`'s values for the levels `from`, laid as `lay`,
/// wrapping or not.
fn displacement(from: [u8; 2], lay: u8, wrap: u8) -> [(&'static str, ParamValue); 6] {
    [
        ("horizontal", ParamValue::Number(23.0)),
        ("vertical", ParamValue::Number(-17.5)),
        ("horizontal-from", ParamValue::Choice(from[0])),
        ("vertical-from", ParamValue::Choice(from[1])),
        ("behaviour", ParamValue::Choice(lay)),
        ("edges", ParamValue::Choice(wrap)),
    ]
}

/// `pattern` made opaque, its colours as they were.
fn opaque(width: u16, height: u16, seed: u64) -> Pixels {
    let pixels = pattern(width, height, seed);
    let data = pixels.data().iter().map(|&[r, g, b, _]| [r, g, b, 1.0]);
    Pixels::from_data(width, height, data.collect())
}

/// An opaque map whose hue never crosses red's, where it is both 0 and 1:
/// where red is the largest colour, green stays above blue.
fn hues(width: u16, height: u16, seed: u64) -> Pixels {
    let pixels = pattern(width, height, seed);
    let data = pixels.data().iter().map(|&[r, g, _, _]| {
        let b = (0.6 * g + 2.0 * (g - r).max(0.0)).min(1.0);
        [r, g, b, 1.0]
    });
    Pixels::from_data(width, height, data.collect())
}

/// The tolerance a displacement reading the levels `from` of a map is
/// held to: an opaque map, or one with transparent areas.
fn tolerance_of(from: [u8; 2], opaque: bool) -> &'static str {
    // Colours read straight, divided by alpha.
    let straight = |level: &u8| *level < 8 && *level != 3;
    match (opaque && !from.contains(&7)) || !from.iter().any(straight) {
        true => "displacement-map",
        // A colour of the map read straight is divided by its alpha, and
        // so is the rounding of a blend of a nearly transparent pixel with
        // its neighbours: at an alpha of 1e-6 a level moves by 1e-3. The
        // saturation of a nearly black colour divides by its lightness the
        // same way.
        false => "displacement-map-straight",
    }
}

/// Every pair of levels the cases read but hue: each level along x,
/// beside another along y.
fn level_pairs() -> impl Iterator<Item = [u8; 2]> {
    (0..11)
        .filter(|&level| level != 5)
        .map(|level| match [level, (level + 3) % 11] {
            [x, 5] => [x, 4],
            pair => pair,
        })
}

#[test]
fn displacement_maps_read_hues_as_on_the_cpu() {
    for (space_name, space) in spaces() {
        for (map, from) in [(hues(183, 119, 53), [5, 0]), (hues(211, 140, 54), [8, 5])] {
            let space = Space {
                input: Some(Input {
                    pixels: map.clone(),
                    bounds: Some(Rect::new(-31.0, 12.5, 452.0, 288.0)),
                }),
                ..space.clone()
            };
            for lay in 0..3 {
                for wrap in 0..2 {
                    let effect = Effect::new("displacement-map", &displacement(from, lay, wrap))
                        .expect("an effect");
                    effect_matches(
                        "displacement-map",
                        &format!(
                            "hue {}x{} {from:?} {lay} {wrap} {space_name}",
                            map.width(),
                            map.height()
                        ),
                        &effect,
                        &pattern(183, 119, 51),
                        &space,
                    );
                }
            }
        }
    }
}

#[test]
fn displacement_maps_read_their_own_pixels_as_on_the_cpu() {
    for (pixels, opaque) in [(pattern(183, 119, 51), false), (opaque(183, 119, 52), true)] {
        for (space_name, space) in spaces() {
            for from in level_pairs() {
                for lay in 0..3 {
                    for wrap in 0..2 {
                        let effect =
                            Effect::new("displacement-map", &displacement(from, lay, wrap))
                                .expect("an effect");
                        effect_matches(
                            tolerance_of(from, opaque),
                            &format!("own {opaque} {from:?} {lay} {wrap} {space_name}"),
                            &effect,
                            &pixels,
                            &space,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn displacement_maps_read_another_layer_as_on_the_cpu() {
    let pixels = pattern(183, 119, 51);
    // A map of its own size, and one larger that the layer reads part of,
    // each with transparent areas and opaque.
    let maps = [
        (pattern(183, 119, 77), None, false),
        (opaque(183, 119, 79), None, true),
        (
            pattern(211, 140, 78),
            Some(Rect::new(-31.0, 12.5, 452.0, 288.0)),
            false,
        ),
        (
            opaque(211, 140, 80),
            Some(Rect::new(-31.0, 12.5, 452.0, 288.0)),
            true,
        ),
    ];
    for (map, bounds, opaque) in maps {
        for (space_name, space) in spaces() {
            let space = Space {
                input: Some(Input {
                    pixels: map.clone(),
                    bounds,
                }),
                ..space
            };
            for from in level_pairs() {
                for lay in 0..3 {
                    for wrap in 0..2 {
                        let effect =
                            Effect::new("displacement-map", &displacement(from, lay, wrap))
                                .expect("an effect");
                        effect_matches(
                            tolerance_of(from, opaque),
                            &format!(
                                "input {}x{} {opaque} {from:?} {lay} {wrap} {space_name}",
                                map.width(),
                                map.height()
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
}

#[test]
fn a_displacement_map_reading_a_layer_runs_on_the_gpu() {
    let Some(gpu) = gpu() else { return };
    let effect = Effect::new("displacement-map", &[])
        .expect("an effect")
        .with_input(Node::Layer(Layer::new(Vec::new())));
    assert!(gpu.runs(&effect));
}

/// `bezier-warp`'s parameters, round the box clockwise from its top-left
/// corner.
const WARP_POINTS: [&str; 12] = [
    "top-left",
    "top-handle-left",
    "top-handle-right",
    "top-right",
    "right-handle-top",
    "right-handle-bottom",
    "bottom-right",
    "bottom-handle-right",
    "bottom-handle-left",
    "bottom-left",
    "left-handle-bottom",
    "left-handle-top",
];

#[test]
fn bezier_warps_match() {
    let alike = [[17.0, -9.5]; 12];
    let mut bent = [[0.0; 2]; 12];
    bent[1] = [-30.0, 80.0];
    bent[2] = [45.0, -60.0];
    bent[4] = [70.0, 25.0];
    bent[8] = [-20.0, -90.0];
    bent[10] = [-55.0, 10.0];
    let mut pinched = [[0.0; 2]; 12];
    pinched[0] = [150.0, 110.0];
    pinched[6] = [-120.0, -95.0];
    pinched[3] = [-60.0, 210.0];
    // The right side pulled past the left, so the patch folds over itself.
    let mut folded = [[0.0; 2]; 12];
    for k in [3, 4, 5, 6] {
        folded[k] = [-520.0, 15.0 * k as f32];
    }
    folded[2] = [-300.0, 40.0];
    folded[7] = [-280.0, -30.0];
    for (case, moves) in [
        ("rest", [[0.0; 2]; 12]),
        ("alike", alike),
        ("bent", bent),
        ("pinched", pinched),
        ("folded", folded),
    ] {
        let values: Vec<(&str, ParamValue)> = WARP_POINTS
            .iter()
            .zip(moves)
            .map(|(name, m)| (*name, ParamValue::Point(m)))
            .collect();
        matches("bezier-warp", "bezier-warp", case, &values);
    }
}

#[test]
fn distortions_hold_where_points_are_as_far_out_as_on_a_4k_frame() {
    // A layer whose box lies 3,000 px out, scaled a little, as the right
    // of a 4K frame.
    let mut far = space(Affine::scale(1.07) * Affine::translate((-3100.0, -1650.0)));
    far.bounds = Some(Rect::new(3080.5, 1640.25, 3290.0, 1771.5));
    let pixels = pattern(229, 143, 61);
    let point = |x: f32, y: f32| ParamValue::Point([3100.0 + x, 1650.0 + y]);
    let cases: [(&str, Vec<(&str, ParamValue)>); 9] = [
        (
            "motion-tile",
            vec![
                ("center", point(40.0, 31.0)),
                ("tile-width", ParamValue::Number(0.37)),
            ],
        ),
        ("wrap", vec![("to", point(61.0, 23.0))]),
        (
            "mirror",
            vec![
                ("center", point(73.0, 51.0)),
                ("angle", ParamValue::Number(37.0)),
            ],
        ),
        (
            "polar-coordinates",
            vec![("interpolation", ParamValue::Number(0.6))],
        ),
        (
            "lens-distortion",
            vec![
                ("field-of-view", ParamValue::Number(121.0)),
                ("center", point(90.0, 60.0)),
            ],
        ),
        (
            "repeat-tile",
            vec![
                ("right", ParamValue::Number(80.0)),
                ("tiling", ParamValue::Choice(1)),
            ],
        ),
        (
            "turbulent-displace",
            vec![
                ("size", ParamValue::Number(31.0)),
                ("complexity", ParamValue::Number(3.5)),
            ],
        ),
        ("displacement-map", displacement([3, 8], 2, 1).to_vec()),
        (
            "bezier-warp",
            vec![
                ("top-handle-left", ParamValue::Point([-30.0, 40.0])),
                ("bottom-right", ParamValue::Point([25.0, -12.0])),
            ],
        ),
    ];
    for (name, values) in cases {
        let effect = Effect::new(name, &values).expect("an effect");
        effect_matches(
            "distort-far",
            &format!("{name} far out"),
            &effect,
            &pixels,
            &far,
        );
    }
}
