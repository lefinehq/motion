//! Unit tests for `src/channel.rs`.

use rstest::rstest;
use vello_cpu::kurbo::{Affine, Rect};

use crate::effect::Input;

use super::*;

use ParamValue::Choice;

/// A 4 by 1 white layer box at the frame's origin, the frame 8 wide.
fn white() -> (Pixels, Space) {
    let mut pixels = Pixels::transparent(8, 1);
    pixels.data_mut()[..4].fill([1.0; 4]);
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: Some(Rect::new(0.0, 0.0, 4.0, 1.0)),
        input: None,
        time: 0.0,
        sound: None,
        outline: None,
        totals: Vec::new(),
    };
    (pixels, space)
}

/// A matte over the layer's box of one pixel each: red, half-transparent
/// white, opaque grey and nothing.
fn matte(space: &mut Space, bounds: Rect) {
    let mut pixels = Pixels::transparent(8, 1);
    pixels.data_mut()[..4].copy_from_slice(&[
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 0.5, 0.5, 0.5],
        [0.25, 0.25, 0.25, 1.0],
        [0.0; 4],
    ]);
    space.input = Some(Input::new(pixels, Some(bounds)));
}

/// The settings in the order of the row, each by its name: channel,
/// inverted, behaviour, composite, premultiply.
fn settings(names: [&str; 5]) -> Vec<ParamValue> {
    let spec = crate::EFFECTS
        .iter()
        .find(|spec| spec.name == "set-matte")
        .expect("a set-matte row");
    spec.params
        .iter()
        .zip(names)
        .map(|(param, name)| {
            let i = param.choices.iter().position(|c| *c == name);
            Choice(i.expect("a choice of the row") as u8)
        })
        .collect()
}

fn alphas(pixels: &Pixels) -> Vec<f32> {
    pixels.data()[..4].iter().map(|p| p[3]).collect()
}

#[rstest]
// Red over black: all of the red, half of the half-transparent white.
#[case::red("red", "on", [1.0, 0.5, 0.25, 0.0])]
// Alpha: the half-transparent white keeps half.
#[case::alpha("alpha", "on", [1.0, 0.5, 1.0, 0.0])]
// Luminance over black reads the half-transparent white as 0.5.
#[case::luminance_over_black("luminance", "on", [0.299, 0.5, 0.25, 0.0])]
// Read straight, the half-transparent white is white.
#[case::luminance_straight("luminance", "off", [0.299, 1.0, 0.25, 0.0])]
#[case::full("full", "on", [1.0; 4])]
#[case::off("off", "on", [0.0; 4])]
fn a_matte_level_becomes_the_layers_alpha(
    #[case] channel: &str,
    #[case] premultiply: &str,
    #[case] expected: [f32; 4],
) {
    let (mut pixels, mut space) = white();
    matte(&mut space, Rect::new(0.0, 0.0, 4.0, 1.0));
    let values = settings([channel, "off", "stretch", "on", premultiply]);
    set_matte(&mut pixels, &values, &space);
    let got = alphas(&pixels);
    assert!(
        got.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-4),
        "{got:?} against {expected:?}"
    );
    // The colour stays white where the layer shows.
    assert!(pixels.data()[..4].iter().all(|p| p[0] == p[3]));
}

#[test]
fn an_inverted_matte_keeps_the_layer_where_the_level_is_low() {
    let (mut pixels, mut space) = white();
    matte(&mut space, Rect::new(0.0, 0.0, 4.0, 1.0));
    set_matte(
        &mut pixels,
        &settings(["red", "on", "stretch", "on", "on"]),
        &space,
    );
    assert_eq!(alphas(&pixels), [0.0, 0.5, 0.75, 1.0]);
}

/// A matte box half the layer's: stretched, each of its pixels covers two
/// of the layer's; centred, it covers the middle two at its own size.
#[test]
fn a_smaller_matte_stretches_over_the_layer_or_sits_in_its_middle() {
    let (mut stretched, mut space) = white();
    let mut half = Pixels::transparent(8, 1);
    half.data_mut()[..2].copy_from_slice(&[[1.0; 4], [0.0, 0.0, 0.0, 1.0]]);
    space.input = Some(Input::new(half, Some(Rect::new(0.0, 0.0, 2.0, 1.0))));
    let mut centred = stretched.clone();
    set_matte(
        &mut stretched,
        &settings(["luminance", "off", "stretch", "on", "on"]),
        &space,
    );
    // Read between the matte's pixels: its white over the first two, its
    // black over the last two, blended where they meet.
    assert_eq!(alphas(&stretched), [0.75, 0.75, 0.25, 0.0]);
    set_matte(
        &mut centred,
        &settings(["luminance", "off", "center", "on", "on"]),
        &space,
    );
    // Centred, the matte's white lands on the layer's second pixel and its
    // black on the third; beyond it there is no matte.
    assert_eq!(alphas(&centred), [0.0, 1.0, 0.0, 0.0]);
}

#[test]
fn without_composite_the_matte_replaces_the_alpha_inside_the_box() {
    let (mut pixels, mut space) = white();
    // The layer is half transparent and drew nothing in its last pixel.
    pixels.data_mut()[..3].fill([0.5; 4]);
    pixels.data_mut()[3] = [0.0; 4];
    let mut full = Pixels::transparent(8, 1);
    full.data_mut().fill([1.0; 4]);
    space.input = Some(Input::new(full, Some(Rect::new(0.0, 0.0, 4.0, 1.0))));
    set_matte(
        &mut pixels,
        &settings(["alpha", "off", "stretch", "off", "on"]),
        &space,
    );
    // White at full alpha where it drew, black where it did not, nothing
    // outside its box.
    assert_eq!(pixels.get(0, 0), [1.0; 4]);
    assert_eq!(pixels.get(3, 0), [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(pixels.get(4, 0), [0.0; 4]);
}

#[test]
fn without_a_matte_layer_the_layer_is_its_own_matte() {
    let (mut pixels, space) = white();
    pixels.data_mut()[1] = [0.5, 0.0, 0.0, 0.5];
    set_matte(
        &mut pixels,
        &settings(["alpha", "off", "stretch", "on", "on"]),
        &space,
    );
    assert_eq!(pixels.get(1, 0), [0.25, 0.0, 0.0, 0.25]);
}

#[rstest]
#[case::red(Level::Red, 0.5)]
#[case::luminance(Level::Luminance, 0.5 * 0.299 + 0.25 * 0.587)]
#[case::hue(Level::Hue, 30.0 / 360.0)]
#[case::lightness(Level::Lightness, 0.25)]
#[case::saturation(Level::Saturation, 1.0)]
fn levels_read_the_colour_as_asked(#[case] level: Level, #[case] over_black: f32) {
    // Orange at half alpha: straight it is twice as bright as over black.
    let pixel = [0.5, 0.25, 0.0, 0.5];
    let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
    assert!(near(level.of(pixel, Reading::OverBlack), over_black));
    let straight = level.of(pixel, Reading::Straight);
    match level {
        Level::Hue | Level::Saturation => assert!(near(straight, over_black)),
        _ => assert!(near(straight, (over_black * 2.0).min(1.0)), "{straight}"),
    }
}

/// The four settings of `shift-channels`, each by its name: alpha, red,
/// green and blue from.
fn shifts(names: [&str; 4]) -> Vec<ParamValue> {
    let spec = crate::EFFECTS
        .iter()
        .find(|spec| spec.name == "shift-channels")
        .expect("a shift-channels row");
    spec.params
        .iter()
        .zip(names)
        .map(|(param, name)| {
            let i = param.choices.iter().position(|c| *c == name);
            Choice(i.expect("a choice of the row") as u8)
        })
        .collect()
}

#[test]
fn shifted_channels_take_levels_of_the_straight_colour() {
    let (mut pixels, space) = white();
    // Half-transparent orange, straight (1, 0.5, 0).
    pixels.data_mut()[0] = [0.5, 0.25, 0.0, 0.5];
    let mut swapped = pixels.clone();
    shift_channels(
        &mut pixels,
        &shifts(["alpha", "off", "off", "blue"]),
        &space,
    );
    // Only blue is left, which this orange has none of.
    assert_eq!(pixels.get(0, 0), [0.0, 0.0, 0.0, 0.5]);
    assert_eq!(pixels.get(1, 0), [0.0, 0.0, 1.0, 1.0]);
    shift_channels(
        &mut swapped,
        &shifts(["red", "green", "red", "alpha"]),
        &space,
    );
    // Alpha from red (1), red from green (0.5), green from red (1), blue
    // from alpha (0.5).
    assert_eq!(swapped.get(0, 0), [0.5, 1.0, 0.5, 1.0]);
}

#[test]
fn a_full_alpha_fills_the_box_with_black_where_the_layer_drew_nothing() {
    let (mut pixels, space) = white();
    pixels.data_mut()[1] = [0.0; 4];
    shift_channels(
        &mut pixels,
        &shifts(["full", "red", "green", "blue"]),
        &space,
    );
    assert_eq!(pixels.get(0, 0), [1.0; 4]);
    assert_eq!(pixels.get(1, 0), [0.0, 0.0, 0.0, 1.0]);
    // Outside the box nothing is drawn.
    assert_eq!(pixels.get(5, 0), [0.0; 4]);
}
