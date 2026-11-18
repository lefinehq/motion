//! The channel effects: `set-matte` by every choice, with its own drawing
//! or another layer's as the matte, with and without a box of its own;
//! `shift-channels` by every level of every channel; and the colour cycle
//! by every way of modifying, adding and interpolating.

use nakkash_raster::kurbo::{Affine, Point, Rect};
use nakkash_raster::{Accelerator, Curve, Effect, Input, Layer, Node, ParamValue, Pixels, Space};

use super::{Random, effect_matches, gpu, pattern, space};

/// The spaces a layer's pixels are in: as drawn, upright and scaled,
/// turned, and without a box.
fn spaces() -> Vec<(&'static str, Space)> {
    let mut drawn = space(Affine::IDENTITY);
    drawn.bounds = Some(Rect::new(8.0, 5.0, 150.0, 100.0));
    let upright = space(Affine::translate((17.5, -9.0)) * Affine::scale_non_uniform(0.45, 0.3));
    let mut turned = space(Affine::rotate_about(0.35, Point::new(90.0, 60.0)) * Affine::scale(0.4));
    turned.bounds = Some(Rect::new(30.0, 25.0, 390.0, 260.0));
    let mut boxless = space(Affine::rotate(0.2) * Affine::scale(1.1));
    boxless.bounds = None;
    vec![
        ("drawn", drawn),
        ("upright", upright),
        ("turned", turned),
        ("boxless", boxless),
    ]
}

/// `space` reading `pixels` as another layer's drawing, with `bounds`.
fn reading(space: &Space, pixels: Pixels, bounds: Option<Rect>) -> Space {
    Space {
        input: Some(Input { pixels, bounds }),
        ..space.clone()
    }
}

/// [`pattern`] with greys, colours whose largest channels tie, where a hue
/// is read off the boundary between its branches, and reds whose green and
/// blue tie, where it wraps from 1 back to 0.
fn with_ties(width: u16, height: u16, seed: u64) -> Pixels {
    let base = pattern(width, height, seed);
    let mut random = Random(seed ^ 0x7135);
    let w = usize::from(width);
    let data = base
        .data()
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let a = p[3];
            let v = random.next();
            match i % w % 11 {
                0 => [v * a, v * a, v * a, a],
                1 => [v * a, v * a, 0.2 * v * a, a],
                2 => [0.3 * v * a, v * a, v * a, a],
                3 => [v * a, 0.4 * v * a, 0.4 * v * a, a],
                _ => p,
            }
        })
        .collect();
    Pixels::from_data(width, height, data)
}

#[test]
fn effects_that_read_a_layer_run_on_the_gpu_with_one() {
    let Some(gpu) = gpu() else { return };
    for name in ["set-matte", "color-cycle", "shift-channels"] {
        let effect = Effect::new(name, &[]).expect("an effect");
        assert!(gpu.runs(&effect), "{name}");
    }
    for name in ["set-matte", "color-cycle"] {
        let effect = Effect::new(name, &[])
            .expect("an effect")
            .with_input(Node::Layer(Layer::new(Vec::new())));
        assert!(gpu.runs(&effect), "{name} reading a layer");
    }
}

#[test]
fn mattes_match_by_every_choice() {
    let (width, height) = (161, 103);
    let pixels = with_ties(width, height, 60);
    let same = with_ties(width, height, 61);
    let smaller = with_ties(97, 71, 62);
    for (space_name, space) in spaces() {
        let mattes = [
            ("its own", space.clone()),
            ("a layer", reading(&space, same.clone(), None)),
            (
                "a smaller layer in a box",
                reading(
                    &space,
                    smaller.clone(),
                    Some(Rect::new(20.0, 5.0, 90.0, 60.0)),
                ),
            ),
        ];
        for (matte_name, space) in &mattes {
            for channel in 0..10 {
                for inverted in 0..2 {
                    for behaviour in 0..2 {
                        for composite in 0..2 {
                            for premultiply in 0..2 {
                                let params = [
                                    ("channel", ParamValue::Choice(channel)),
                                    ("inverted", ParamValue::Choice(inverted)),
                                    ("behaviour", ParamValue::Choice(behaviour)),
                                    ("composite", ParamValue::Choice(composite)),
                                    ("premultiply", ParamValue::Choice(premultiply)),
                                ];
                                let effect = Effect::new("set-matte", &params).expect("a matte");
                                if !runs_on_the_gpu(channel, premultiply) {
                                    if let Some(gpu) = gpu() {
                                        assert!(!gpu.runs(&effect), "{params:?}");
                                    }
                                    continue;
                                }
                                let case =
                                    format!("{params:?} {space_name}, the matte {matte_name}");
                                effect_matches("set-matte", &case, &effect, &pixels, space);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_matte_stretched_over_a_wide_layer_matches_far_from_the_origin() {
    // Points of the matte are worked out in f32 on the GPU: at 1900 px
    // from the origin a step of an f32 is 1.2e-4 px.
    let pixels = with_ties(1931, 23, 67);
    let matte = with_ties(1717, 19, 68);
    let mut wide = space(Affine::translate((3.25, 0.5)));
    wide.bounds = Some(Rect::new(0.0, 0.0, 1925.0, 22.0));
    let space = reading(&wide, matte, Some(Rect::new(4.0, 1.0, 1710.0, 18.0)));
    for (channel, behaviour) in [(0, 0), (4, 0), (0, 1), (1, 1)] {
        let effect = Effect::new(
            "set-matte",
            &[
                ("channel", ParamValue::Choice(channel)),
                ("behaviour", ParamValue::Choice(behaviour)),
            ],
        )
        .expect("a matte");
        effect_matches(
            "set-matte",
            &format!("channel {channel} behaviour {behaviour}, wide"),
            &effect,
            &pixels,
            &space,
        );
    }
}

/// Whether a matte of `channel`, read with `premultiply`, runs on the GPU:
/// alpha, full and off always; hue (5) and saturation (7) never; the
/// other levels of the colour laid over black (`premultiply` on, 0).
fn runs_on_the_gpu(channel: u8, premultiply: u8) -> bool {
    match channel {
        0 | 8 | 9 => true,
        5 | 7 => false,
        _ => premultiply == 0,
    }
}

#[test]
fn a_matte_read_off_its_colour_s_proportions_stays_on_the_cpu() {
    let Some(gpu) = gpu() else { return };
    for (channel, premultiply) in [(5, 0), (7, 0), (1, 1), (4, 1), (6, 1)] {
        let effect = Effect::new(
            "set-matte",
            &[
                ("channel", ParamValue::Choice(channel)),
                ("premultiply", ParamValue::Choice(premultiply)),
            ],
        )
        .expect("a matte");
        assert!(
            !gpu.runs(&effect),
            "channel {channel} premultiply {premultiply}"
        );
    }
}

/// The levels `shift-channels` offers each channel.
const SHIFT_CHOICES: u8 = 10;

#[test]
fn shifted_channels_match_by_every_level() {
    let pixels = with_ties(149, 87, 63);
    let names = ["alpha-from", "red-from", "green-from", "blue-from"];
    let mut random = Random(64);
    let mut settings: Vec<[u8; 4]> = Vec::new();
    // Each channel from each of its levels, the others as they are.
    for (c, _) in names.iter().enumerate() {
        for choice in 0..SHIFT_CHOICES {
            let mut setting = [0; 4];
            setting[c] = choice;
            settings.push(setting);
        }
    }
    // And mixtures, some with a full alpha.
    for k in 0..24 {
        let mut setting: [u8; 4] =
            std::array::from_fn(|_| (random.next() * f32::from(SHIFT_CHOICES)) as u8);
        if k % 3 == 0 {
            setting[0] = 8;
        }
        settings.push(setting);
    }
    for (space_name, space) in spaces() {
        for setting in &settings {
            let params: Vec<(&str, ParamValue)> = names
                .iter()
                .zip(setting)
                .map(|(&name, &choice)| (name, ParamValue::Choice(choice)))
                .collect();
            let effect = Effect::new("shift-channels", &params).expect("a shift");
            // Hue (choice 5 of every channel) jumps from 1 back to 0 at red.
            let row = match setting.contains(&5) {
                true => "shift-channels-hue",
                false => "shift-channels",
            };
            effect_matches(
                row,
                &format!("{setting:?} {space_name}"),
                &effect,
                &pixels,
                &space,
            );
        }
    }
}

fn curve(points: &[[f32; 2]]) -> ParamValue {
    ParamValue::Curve(Curve::new(points).expect("a curve"))
}

#[test]
fn colour_cycles_match_by_every_choice() {
    let (width, height) = (127, 83);
    let pixels = pattern(width, height, 65);
    let added = pattern(width, height, 66);
    let cycle = [
        curve(&[[0.0, 0.2], [0.3, 0.9], [0.7, 0.1]]),
        curve(&[[0.1, 1.0], [0.6, 0.0]]),
        curve(&[]),
        curve(&[[0.25, 0.5], [0.5, 0.8], [0.9, 0.3]]),
    ];
    let full: Vec<[f32; 2]> = (0..Curve::MAX)
        .map(|k| {
            let x = k as f32 / Curve::MAX as f32;
            [x, (x * 7.0).fract()]
        })
        .collect();
    let drawn = space(Affine::IDENTITY);
    let spaces = [
        ("alone", drawn.clone()),
        ("adding a layer", reading(&drawn, added, None)),
    ];
    for (space_name, space) in &spaces {
        for modify in 0..14u8 {
            for add_mode in 0..4u8 {
                for interpolate in 0..2u8 {
                    let curves = match modify % 2 {
                        0 => cycle,
                        _ => [curve(&full), cycle[2], cycle[0], curve(&full)],
                    };
                    let params = [
                        ("red", curves[0]),
                        ("green", curves[1]),
                        ("blue", curves[2]),
                        ("alpha", curves[3]),
                        (
                            "shift",
                            ParamValue::Number(37.0 + 50.0 * f32::from(add_mode)),
                        ),
                        ("repetitions", ParamValue::Number(1.5)),
                        ("blend", ParamValue::Number(0.2)),
                        ("phase-from", ParamValue::Choice(modify % 10)),
                        ("add-from", ParamValue::Choice((modify + 3) % 10)),
                        ("add-mode", ParamValue::Choice(add_mode)),
                        ("modify", ParamValue::Choice(modify)),
                        ("interpolate", ParamValue::Choice(interpolate)),
                        ("modify-alpha", ParamValue::Choice(modify % 2)),
                    ];
                    let effect = Effect::new("color-cycle", &params).expect("a cycle");
                    let row = match interpolate {
                        0 => "color-cycle",
                        _ => "color-cycle-held",
                    };
                    let case = format!(
                        "modify {modify} add-mode {add_mode} interpolate {interpolate} {space_name}"
                    );
                    effect_matches(row, &case, &effect, &pixels, space);
                }
            }
        }
    }
}
