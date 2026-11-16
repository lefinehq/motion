//! Unit tests for `src/colour.rs`.

use vello_cpu::kurbo::Affine;

use super::*;
use crate::style::Color;

/// One pixel of straight `rgba`, premultiplied.
fn one(rgba: [f32; 4]) -> Pixels {
    let mut pixels = Pixels::transparent(1, 1);
    let a = rgba[3];
    pixels.data_mut()[0] = [rgba[0] * a, rgba[1] * a, rgba[2] * a, a];
    pixels
}

/// The straight colour of `rgba` after `change` with `values`.
fn after(
    change: fn(&mut Pixels, &[ParamValue], &Space),
    values: &[ParamValue],
    rgba: [f32; 4],
) -> [f32; 4] {
    let mut pixels = one(rgba);
    change(
        &mut pixels,
        values,
        &Space {
            transform: Affine::IDENTITY,
            bounds: None,
            input: None,
            time: 0.0,
            sound: None,
            outline: None,
        },
    );
    straight(pixels.data()[0])
}

fn close(got: [f32; 4], want: [f32; 4]) {
    assert!(
        got.iter().zip(&want).all(|(g, w)| (g - w).abs() < 2e-3),
        "{got:?} is not {want:?}"
    );
}

use ParamValue::{Choice, Number};

fn color(r: f32, g: f32, b: f32) -> ParamValue {
    ParamValue::Color(Color::rgba(r, g, b, 1.0))
}

#[test]
fn tint_maps_luma_between_black_and_white() {
    let red_to_blue = [color(1.0, 0.0, 0.0), color(0.0, 0.0, 1.0), Number(1.0)];
    close(
        after(tint, &red_to_blue, [1.0, 1.0, 1.0, 1.0]),
        [0.0, 0.0, 1.0, 1.0],
    );
    close(
        after(tint, &red_to_blue, [0.0, 0.0, 0.0, 0.5]),
        [1.0, 0.0, 0.0, 0.5],
    );
    let half = [color(0.0, 0.0, 0.0), color(1.0, 1.0, 1.0), Number(0.5)];
    // Green's luma is 0.587; half of the way from green to that grey.
    close(
        after(tint, &half, [0.0, 1.0, 0.0, 1.0]),
        [0.2935, 0.7935, 0.2935, 1.0],
    );
}

#[test]
fn transparent_pixels_stay_transparent() {
    let values = [color(1.0, 0.0, 0.0), color(1.0, 1.0, 1.0), Number(1.0)];
    assert_eq!(after(tint, &values, [0.0; 4]), [0.0; 4]);
}

#[test]
fn hue_saturation_turns_hues_and_moves_saturation_and_lightness() {
    let adjust = |hue: f32, saturation: f32, lightness: f32| {
        [
            Number(hue),
            Number(saturation),
            Number(lightness),
            Number(0.0),
            Number(0.25),
            Number(0.0),
            Choice(0),
        ]
    };
    close(
        after(
            hue_saturation,
            &adjust(120.0, 0.0, 0.0),
            [1.0, 0.0, 0.0, 1.0],
        ),
        [0.0, 1.0, 0.0, 1.0],
    );
    close(
        after(
            hue_saturation,
            &adjust(0.0, -1.0, 0.0),
            [1.0, 0.0, 0.0, 1.0],
        ),
        [0.5, 0.5, 0.5, 1.0],
    );
    close(
        after(hue_saturation, &adjust(0.0, 0.0, 1.0), [0.2, 0.4, 0.6, 1.0]),
        [1.0, 1.0, 1.0, 1.0],
    );
    close(
        after(
            hue_saturation,
            &adjust(0.0, 0.0, -0.5),
            [0.2, 0.4, 0.6, 1.0],
        ),
        [0.1, 0.2, 0.3, 1.0],
    );
    let colorize = [
        Number(0.0),
        Number(0.0),
        Number(0.0),
        Number(240.0),
        Number(1.0),
        Number(0.0),
        Choice(1),
    ];
    // White's luma is 1, so any hue at full lightness is white; grey at
    // half is the pure hue.
    close(
        after(hue_saturation, &colorize, [1.0, 1.0, 1.0, 1.0]),
        [1.0, 1.0, 1.0, 1.0],
    );
    close(
        after(hue_saturation, &colorize, [0.5, 0.5, 0.5, 1.0]),
        [0.0, 0.0, 1.0, 1.0],
    );
}

#[test]
fn exposure_scales_light_by_stops() {
    let gamma_space = [Number(1.0), Number(0.0), Number(1.0), Choice(1)];
    close(
        after(exposure, &gamma_space, [0.25, 0.5, 0.75, 1.0]),
        [0.5, 1.0, 1.0, 1.0],
    );
    // In linear light, one stop doubles the light, not the encoded level.
    let linear = [Number(1.0), Number(0.0), Number(1.0), Choice(0)];
    let doubled = encode(decode(0.5) * 2.0);
    close(
        after(exposure, &linear, [0.5, 0.5, 0.5, 1.0]),
        [doubled, doubled, doubled, 1.0],
    );
    assert!((doubled - 0.686).abs() < 1e-3);
    let nothing = [Number(0.0), Number(0.0), Number(1.0), Choice(0)];
    close(
        after(exposure, &nothing, [0.3, 0.6, 0.9, 1.0]),
        [0.3, 0.6, 0.9, 1.0],
    );
}

#[test]
fn levels_map_between_ranges_on_their_channel() {
    let values = |channel: u8| {
        [
            Number(0.2),
            Number(0.6),
            Number(1.0),
            Number(0.0),
            Number(1.0),
            Choice(channel),
        ]
    };
    close(
        after(levels, &values(0), [0.4, 0.1, 0.8, 1.0]),
        [0.5, 0.0, 1.0, 1.0],
    );
    close(
        after(levels, &values(1), [0.4, 0.1, 0.8, 1.0]),
        [0.5, 0.1, 0.8, 1.0],
    );
    let gamma = [
        Number(0.0),
        Number(1.0),
        Number(2.0),
        Number(0.0),
        Number(1.0),
        Choice(0),
    ];
    close(
        after(levels, &gamma, [0.25, 0.25, 0.25, 1.0]),
        [0.5, 0.5, 0.5, 1.0],
    );
}

#[test]
fn invert_inverts_its_channel_and_keeps_its_blend() {
    close(
        after(invert, &[Number(0.0), Choice(0)], [0.2, 0.4, 1.0, 1.0]),
        [0.8, 0.6, 0.0, 1.0],
    );
    close(
        after(invert, &[Number(0.5), Choice(3)], [0.2, 0.4, 1.0, 1.0]),
        [0.2, 0.4, 0.5, 1.0],
    );
    close(
        after(invert, &[Number(0.0), Choice(4)], [0.2, 0.4, 1.0, 0.25]),
        [0.2, 0.4, 1.0, 0.75],
    );
}

fn curve(points: &[[f32; 2]]) -> ParamValue {
    ParamValue::Curve(Curve::new(points).unwrap())
}

#[test]
fn curves_run_each_channels_curve_then_rgb() {
    let straight = curve(&[]);
    let lift_red = [
        straight,
        curve(&[[0.0, 0.2], [1.0, 1.0]]),
        straight,
        straight,
        straight,
    ];
    close(
        after(curves, &lift_red, [0.0, 0.0, 0.0, 1.0]),
        [51.0 / 255.0, 0.0, 0.0, 1.0],
    );
    // Red lifted to 0.2, then rgb halves every level.
    let then_halve = [
        curve(&[[0.0, 0.0], [1.0, 0.5]]),
        lift_red[1],
        straight,
        straight,
        straight,
    ];
    close(
        after(curves, &then_halve, [0.0, 0.0, 1.0, 1.0]),
        [26.0 / 255.0, 0.0, 128.0 / 255.0, 1.0],
    );
    let identity = [straight; 5];
    close(
        after(curves, &identity, [0.3, 0.6, 0.9, 0.5]),
        [0.3, 0.6, 0.9, 0.5],
    );
}

#[test]
fn curves_are_flat_beyond_their_ends() {
    let middle = curve(&[[0.25, 0.1], [0.75, 0.8]]);
    let straight = curve(&[]);
    let values = [middle, straight, straight, straight, straight];
    let low = 26.0 / 255.0;
    let high = 204.0 / 255.0;
    close(
        after(curves, &values, [0.0, 0.2, 1.0, 1.0]),
        [low, low, high, 1.0],
    );
}

#[test]
fn a_natural_spline_bends_through_its_points() {
    let spline = Spline::new(&[[0.0, 0.0], [0.25, 0.4], [1.0, 1.0]]);
    assert_eq!(spline.at(0.0), 0.0);
    assert!((spline.at(0.25) - 0.4).abs() < 1e-6);
    assert_eq!(spline.at(1.0), 1.0);
    // A straight line through three points stays straight.
    let line = Spline::new(&[[0.0, 0.0], [0.4, 0.4], [1.0, 1.0]]);
    assert!((0..=10).all(|i| (line.at(f64::from(i) / 10.0) - f64::from(i) / 10.0).abs() < 1e-9));
}

#[test]
fn black_white_turns_each_hue_as_light_as_its_share() {
    let defaults: Vec<ParamValue> = [0.4, 0.6, 0.4, 0.6, 0.2, 0.8, 0.0]
        .map(Number)
        .into_iter()
        .chain([color(0.88, 0.83, 0.7)])
        .collect();
    let grey = |rgba| after(black_white, &defaults, rgba);
    close(grey([1.0, 1.0, 1.0, 1.0]), [1.0, 1.0, 1.0, 1.0]);
    close(grey([1.0, 0.0, 0.0, 1.0]), [0.4, 0.4, 0.4, 1.0]);
    close(grey([1.0, 1.0, 0.0, 1.0]), [0.6, 0.6, 0.6, 1.0]);
    close(grey([1.0, 0.0, 1.0, 0.5]), [0.8, 0.8, 0.8, 0.5]);
    // Orange: its lowest is 0, red over yellow 0.5 at 0.4, yellow 0.5 at 0.6.
    close(grey([1.0, 0.5, 0.0, 1.0]), [0.5, 0.5, 0.5, 1.0]);
    let mut tinted = defaults.clone();
    tinted[6] = Number(1.0);
    tinted[7] = color(0.0, 0.0, 1.0);
    // Mid grey tinted pure blue is the pure hue at half lightness.
    close(
        after(black_white, &tinted, [0.5, 0.5, 0.5, 1.0]),
        [0.0, 0.0, 1.0, 1.0],
    );
}
