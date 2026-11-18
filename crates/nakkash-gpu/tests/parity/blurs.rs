//! The blurs that gather along lines and discs, and the bevel: the radial
//! blur's zoom and spin by every stretch and fade, from centres inside, on
//! the edge of and outside the grid; the vector blur by every streak and
//! level, with and without a map; the lens blur by every iris, edge and
//! map placement; the bevel by thickness and light. Each at the largest
//! amounts it takes, on odd and large grids.

use super::*;

/// A radial blur of `amount` about `centre`: `spin` or a zoom, `across`
/// or behind, fading or flat.
fn radial(amount: f32, centre: [f32; 2], spin: bool, across: bool, linear: bool) -> Effect {
    Effect::new(
        "radial-blur",
        &[
            ("amount", ParamValue::Number(amount)),
            ("center", ParamValue::Point(centre)),
            ("type", ParamValue::Choice(u8::from(spin))),
            ("stretch", ParamValue::Choice(u8::from(across))),
            ("fade", ParamValue::Choice(u8::from(linear))),
        ],
    )
    .expect("a radial blur")
}

/// Every stretch and fade of one kind, amount and centre.
fn radial_cases(spin: bool, amount: f32, centre: [f32; 2], pixels: &Pixels, space: &Space) {
    let name = if spin { "radial-spin" } else { "radial-zoom" };
    for across in [false, true] {
        for linear in [false, true] {
            effect_matches(
                name,
                &format!(
                    "{amount} at {centre:?} across {across} linear {linear} {}x{}",
                    pixels.width(),
                    pixels.height()
                ),
                &radial(amount, centre, spin, across, linear),
                pixels,
                space,
            );
        }
    }
}

const CENTRES: [[f32; 2]; 5] = [
    // Inside, on a pixel's corner, on the grid's edge, outside beside it
    // and far outside past a corner.
    [61.3, 47.8],
    [90.0, 40.0],
    [0.0, 70.5],
    [-35.2, 51.7],
    [420.6, -310.2],
];

#[test]
fn zooms_match() {
    let pixels = pattern(181, 117, 50);
    let one = space(Affine::IDENTITY);
    for centre in CENTRES {
        for amount in [4.0, 35.0, -60.0, 100.0, 3_600.0, -3_600.0] {
            radial_cases(false, amount, centre, &pixels, &one);
        }
    }
}

#[test]
fn spins_match() {
    let pixels = pattern(181, 117, 51);
    let one = space(Affine::IDENTITY);
    for centre in CENTRES {
        for amount in [2.0, 17.0, -45.0, 360.0, 3_600.0, -3_600.0] {
            radial_cases(true, amount, centre, &pixels, &one);
        }
    }
}

#[test]
fn radial_blurs_match_through_a_turned_scaled_layer() {
    let pixels = pattern(97, 203, 52);
    let turned = space(
        Affine::translate((20.5, -14.0))
            * Affine::rotate(0.6)
            * Affine::scale_non_uniform(1.3, 0.7),
    );
    for spin in [false, true] {
        radial_cases(spin, 40.0, [30.0, 60.0], &pixels, &turned);
    }
}

#[test]
fn radial_blurs_match_on_a_large_grid() {
    // Far from the centre a float holds a px to a few 10^-4: what the
    // kernels split into a whole px and a part is measured here.
    let pixels = pattern(1_920, 1_080, 53);
    let one = space(Affine::IDENTITY);
    for (spin, amount) in [(false, 2.0), (false, 100.0), (true, 1.0), (true, 30.0)] {
        for centre in [[13.4, 21.9], [2_400.0, 1_500.0]] {
            radial_cases(spin, amount, centre, &pixels, &one);
        }
    }
}

#[test]
fn radial_blurs_match_on_a_4k_grid() {
    // A 4K grid's rays and rings are read in several batches.
    let pixels = pattern(3_840, 2_160, 56);
    let one = space(Affine::IDENTITY);
    for (spin, amount, centre) in [
        (false, 60.0, [1_700.3, 980.6]),
        (true, 1.5, [-20.0, 2_200.0]),
    ] {
        let name = if spin { "radial-spin" } else { "radial-zoom" };
        let effect = radial(amount, centre, spin, false, true);
        effect_matches(
            name,
            &format!("{amount} at {centre:?} 4K"),
            &effect,
            &pixels,
            &one,
        );
    }
}

#[test]
fn radial_blurs_of_nothing_change_nothing() {
    let pixels = pattern(64, 33, 54);
    for spin in [false, true] {
        radial_cases(spin, 0.0, [10.0, 10.0], &pixels, &space(Affine::IDENTITY));
    }
    // A grid of one px.
    radial_cases(
        false,
        50.0,
        [0.2, 0.7],
        &pattern(1, 1, 55),
        &space(Affine::IDENTITY),
    );
    radial_cases(
        true,
        50.0,
        [0.2, 0.7],
        &pattern(1, 1, 55),
        &space(Affine::IDENTITY),
    );
}

/// A vector blur by `streak` along the slope of `level`.
fn vector(amount: f32, angle: f32, ridge: f32, softness: f32, streak: u8, level: u8) -> Effect {
    Effect::new(
        "vector-blur",
        &[
            ("amount", ParamValue::Number(amount)),
            ("angle", ParamValue::Number(angle)),
            ("ridge-smoothness", ParamValue::Number(ridge)),
            ("softness", ParamValue::Number(softness)),
            ("type", ParamValue::Choice(streak)),
            ("map-level", ParamValue::Choice(level)),
        ],
    )
    .expect("a vector blur")
}

/// Checks `vector(...)` on `pixels` in `space`, by the row its softness
/// puts it in.
fn vector_matches(case: &str, effect: &Effect, softness: f32, pixels: &Pixels, space: &Space) {
    let name = match softness > 0.0 {
        true => "vector-blur-softened",
        false => "vector-blur",
    };
    effect_matches(name, case, effect, pixels, space);
}

#[test]
fn vector_blurs_match_by_every_streak_and_level() {
    let pixels = pattern(181, 117, 60);
    let one = space(Affine::IDENTITY);
    for streak in 0..5 {
        for level in 0..8 {
            for (ridge, softness) in [(1.0, 0.0), (0.0, 0.0), (1.0, 6.0)] {
                vector_matches(
                    &format!("streak {streak} level {level} ridge {ridge} softness {softness}"),
                    &vector(12.0, 20.0, ridge, softness, streak, level),
                    softness,
                    &pixels,
                    &one,
                );
            }
        }
    }
}

#[test]
fn vector_blurs_match_at_their_extremes() {
    let pixels = pattern(129, 97, 61);
    let one = space(Affine::IDENTITY);
    for (amount, angle, ridge, softness) in [
        (4_000.0, 0.0, 0.0, 0.0),
        (-4_000.0, 135.0, 30.0, 0.0),
        (60.0, 400.0, 1_000.0, 0.0),
        (0.3, 0.0, 1.0, 0.0),
        (-25.0, -90.0, 0.0, 40.0),
        (0.3, 0.0, 1.0, 15.0),
    ] {
        for streak in 0..5 {
            vector_matches(
                &format!("{amount} {angle} {ridge} {softness} streak {streak}"),
                &vector(amount, angle, ridge, softness, streak, 0),
                softness,
                &pixels,
                &one,
            );
        }
    }
}

#[test]
fn vector_blurs_match_on_a_large_grid() {
    let pixels = pattern(1_920, 1_080, 64);
    let one = space(Affine::IDENTITY);
    for streak in [0, 3] {
        vector_matches(
            &format!("1080p streak {streak}"),
            &vector(40.0, 10.0, 2.0, 0.0, streak, 5),
            0.0,
            &pixels,
            &one,
        );
    }
}

#[test]
fn vector_blurs_match_by_a_map_through_a_scaled_layer() {
    let pixels = pattern(97, 203, 62);
    let mut mapped = space(Affine::translate((3.5, -2.0)) * Affine::scale(1.7));
    mapped.input = Some(nakkash_raster::Input {
        pixels: pattern(97, 203, 63),
        bounds: None,
    });
    for streak in 0..5 {
        for level in [0, 4, 6] {
            for softness in [0.0, 4.0] {
                vector_matches(
                    &format!("mapped streak {streak} level {level} softness {softness}"),
                    &vector(9.0, 30.0, 2.0, softness, streak, level),
                    softness,
                    &pixels,
                    &mapped,
                );
            }
        }
    }
    // A flat map: no slope anywhere.
    let mut flat = space(Affine::IDENTITY);
    flat.input = Some(nakkash_raster::Input {
        pixels: Pixels::from_data(97, 203, vec![[0.2, 0.3, 0.4, 1.0]; 97 * 203]),
        bounds: None,
    });
    for streak in 0..5 {
        vector_matches(
            &format!("flat streak {streak}"),
            &vector(9.0, 0.0, 1.0, 1.0, streak, 0),
            1.0,
            &pixels,
            &flat,
        );
    }
}

/// A lens blur of `radius` by `iris`, with the rest of its parameters.
#[allow(clippy::too_many_arguments)]
fn lens(
    radius: f32,
    roundness: f32,
    aspect: f32,
    rotation: f32,
    iris: u8,
    repeat: bool,
    level: u8,
    stretch: bool,
    inverted: bool,
    focal: f32,
) -> Effect {
    Effect::new(
        "lens-blur",
        &[
            ("radius", ParamValue::Number(radius)),
            ("roundness", ParamValue::Number(roundness)),
            ("aspect", ParamValue::Number(aspect)),
            ("rotation", ParamValue::Number(rotation)),
            ("focal-distance", ParamValue::Number(focal)),
            ("iris", ParamValue::Choice(iris)),
            ("map-level", ParamValue::Choice(level)),
            ("placement", ParamValue::Choice(u8::from(stretch))),
            ("inverted", ParamValue::Choice(u8::from(inverted))),
            ("edges", ParamValue::Choice(u8::from(repeat))),
        ],
    )
    .expect("a lens blur")
}

#[test]
fn lens_blurs_match_by_every_iris_and_edge() {
    let pixels = pattern(181, 117, 70);
    let one = space(Affine::IDENTITY);
    for iris in 0..8 {
        for repeat in [false, true] {
            for (radius, roundness, aspect, rotation) in [
                (0.3, 0.0, 1.0, 0.0),
                (4.6, 0.4, 1.0, 17.0),
                (13.0, 1.0, 2.5, -40.0),
            ] {
                effect_matches(
                    "lens-blur",
                    &format!(
                        "iris {iris} repeat {repeat} radius {radius} {roundness} {aspect} {rotation}"
                    ),
                    &lens(
                        radius, roundness, aspect, rotation, iris, repeat, 0, false, false, 0.0,
                    ),
                    &pixels,
                    &one,
                );
            }
        }
    }
}

#[test]
fn lens_blurs_match_at_their_extremes() {
    let pixels = pattern(97, 63, 71);
    let one = space(Affine::IDENTITY);
    for (radius, aspect) in [
        (0.04, 1.0),
        (0.05, 1.0),
        (300.0, 1.0),
        (4_000.0, 1.0),
        (20.0, 0.01),
        (2.0, 100.0),
    ] {
        for repeat in [false, true] {
            effect_matches(
                "lens-blur",
                &format!("radius {radius} aspect {aspect} repeat {repeat}"),
                &lens(radius, 0.3, aspect, 10.0, 2, repeat, 0, false, false, 0.0),
                &pixels,
                &one,
            );
        }
    }
}

#[test]
fn lens_blurs_match_on_a_large_grid() {
    // Running sums of 1,920 px reach a few hundred, where a float holds
    // them to 3e-5: they must round as the CPU's do.
    let pixels = pattern(1_920, 1_080, 72);
    let one = space(Affine::IDENTITY);
    for radius in [0.7, 6.0] {
        effect_matches(
            "lens-blur",
            &format!("1080p radius {radius}"),
            &lens(radius, 0.5, 1.0, 0.0, 0, false, 0, false, false, 0.0),
            &pixels,
            &one,
        );
    }
}

#[test]
fn lens_blurs_match_by_a_map() {
    let pixels = pattern(131, 89, 73);
    let map = pattern(131, 89, 74);
    let mut scaled = space(Affine::translate((6.5, -3.0)) * Affine::scale(1.3));
    scaled.bounds = Some(nakkash_raster::kurbo::Rect::new(10.0, 5.0, 90.0, 60.0));
    scaled.input = Some(nakkash_raster::Input {
        pixels: map.clone(),
        bounds: Some(nakkash_raster::kurbo::Rect::new(0.0, 0.0, 120.0, 70.0)),
    });
    let mut frame = space(Affine::IDENTITY);
    frame.bounds = None;
    frame.input = Some(nakkash_raster::Input {
        pixels: map,
        bounds: None,
    });
    for (name, space) in [("scaled", &scaled), ("frame", &frame)] {
        for level in 0..5 {
            for (stretch, inverted, focal) in [(false, false, 0.0), (true, true, 0.4)] {
                effect_matches(
                    "lens-blur",
                    &format!("{name} level {level} stretch {stretch} inverted {inverted}"),
                    &lens(
                        9.0,
                        0.2,
                        1.4,
                        25.0,
                        0,
                        level == 2,
                        level,
                        stretch,
                        inverted,
                        focal,
                    ),
                    &pixels,
                    space,
                );
            }
        }
    }
}

/// A bevel `thickness` px thick, lit from `angle`.
fn bevel(thickness: f32, angle: f32, intensity: f32) -> Effect {
    Effect::new(
        "bevel",
        &[
            ("thickness", ParamValue::Number(thickness)),
            ("angle", ParamValue::Number(angle)),
            (
                "color",
                ParamValue::Color(nakkash_raster::Color::rgba(1.0, 0.9, 0.6, 1.0)),
            ),
            ("intensity", ParamValue::Number(intensity)),
        ],
    )
    .expect("a bevel")
}

#[test]
fn bevels_match() {
    let pixels = pattern(181, 117, 80);
    let one = space(Affine::IDENTITY);
    let turned = space(Affine::rotate(0.3) * Affine::scale(1.6));
    for (thickness, angle, intensity) in [
        (1.0, 0.0, 1.0),
        (4.5, 135.0, 0.7),
        (12.0, -60.0, 0.4),
        (200.0, 300.0, 1.0),
        (0.0, 0.0, 1.0),
        (5.0, 0.0, 0.0),
    ] {
        for (name, space) in [("plain", &one), ("turned", &turned)] {
            effect_matches(
                "bevel",
                &format!("{thickness} {angle} {intensity} {name}"),
                &bevel(thickness, angle, intensity),
                &pixels,
                space,
            );
        }
    }
    // All inside: no outline anywhere.
    let full = Pixels::from_data(37, 29, vec![[0.4, 0.5, 0.6, 1.0]; 37 * 29]);
    effect_matches("bevel", "no outline", &bevel(6.0, 45.0, 1.0), &full, &one);
}

#[test]
fn bevels_match_on_a_large_grid() {
    let pixels = pattern(1_920, 1_080, 81);
    effect_matches(
        "bevel",
        "1080p",
        &bevel(30.0, 120.0, 0.8),
        &pixels,
        &space(Affine::IDENTITY),
    );
}
