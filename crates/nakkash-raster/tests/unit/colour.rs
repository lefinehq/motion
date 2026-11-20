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
            totals: Vec::new(),
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

/// What the straight grey level `x` becomes by `brightness-contrast` with
/// `brightness`, `contrast` and `legacy` (0 off, 1 on).
fn brightness_contrast_of(brightness: f32, contrast: f32, legacy: u8, x: f32) -> f32 {
    let values = [Number(brightness), Number(contrast), Choice(legacy)];
    after(brightness_contrast, &values, [x, x, x, 1.0])[0]
}

/// The original's 8-bit levels for grey ramps through the curves, measured
/// by EffectCraft (`crates/effects/src/color_fx/tests.rs`,
/// `brightness_contrast_modern_matches_ae_ramp_samples` and
/// `negative_brightness_and_contrast_invert_the_positive_curves`):
/// brightness and contrast as the original's sliders, the input level and
/// what the original drew.
const MEASURED_LEVELS: [(f32, f32, u8, u8); 19] = [
    (0.0, 100.0, 64, 40),
    (0.0, 100.0, 128, 129),
    (0.0, 100.0, 192, 217),
    (0.0, 50.0, 64, 53),
    (0.0, 50.0, 192, 205),
    (0.0, -50.0, 64, 77),
    (100.0, 0.0, 128, 209),
    (50.0, 0.0, 128, 171),
    (106.0, -18.0, 128, 214),
    (-40.0, 0.0, 64, 50),
    (-40.0, 0.0, 128, 99),
    (-40.0, 0.0, 191, 155),
    (-40.0, 0.0, 234, 207),
    (0.0, -50.0, 21, 28),
    (0.0, -50.0, 64, 76),
    (0.0, -50.0, 191, 179),
    (0.0, -50.0, 234, 227),
    (40.0, 0.0, 128, 162),
    (0.0, 50.0, 64, 52),
];

#[test]
fn brightness_contrast_curves_draw_the_measured_levels_within_one_step() {
    let mut worst = 0.0f32;
    let mut sum = 0.0;
    for (brightness, contrast, input, measured) in MEASURED_LEVELS {
        let x = f32::from(input) / 255.0;
        let level = brightness_contrast_of(brightness / 100.0, contrast / 100.0, 0, x) * 255.0;
        let off = (level - f32::from(measured)).abs();
        assert!(
            (level.round() - f32::from(measured)).abs() <= 1.0,
            "brightness {brightness} contrast {contrast} at {input}: {level} for {measured}"
        );
        worst = worst.max(off);
        sum += off;
    }
    // The fit: at most 1.14 of a step off, 0.54 on average.
    assert!(worst < 1.15, "{worst}");
    assert!(sum / 19.0 < 0.55, "{}", sum / 19.0);
}

#[test]
fn darkening_curves_invert_the_brightening_ones() {
    for brightness in [0.1, 0.4, 1.0, 1.5] {
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            let up = brightness_contrast_of(brightness, 0.0, 0, x);
            let back = brightness_contrast_of(-brightness, 0.0, 0, up);
            assert!((back - x).abs() < 1e-4, "{brightness} at {x}: {back}");
        }
    }
}

#[test]
fn brightness_contrast_curves_keep_black_white_and_their_order() {
    for brightness in [-1.5, -0.7, 0.0, 0.3, 1.5] {
        for contrast in [-1.0, -0.5, 0.0, 0.6, 1.0] {
            assert_eq!(brightness_contrast_of(brightness, contrast, 0, 0.0), 0.0);
            assert!((brightness_contrast_of(brightness, contrast, 0, 1.0) - 1.0).abs() < 1e-6);
            let mut last = 0.0;
            for i in 1..=100 {
                let level = brightness_contrast_of(brightness, contrast, 0, i as f32 / 100.0);
                assert!(level >= last, "{brightness} {contrast} at {i}");
                last = level;
            }
        }
    }
}

#[test]
fn contrast_below_a_half_draws_as_a_half() {
    for x in [0.1, 0.3, 0.8] {
        assert_eq!(
            brightness_contrast_of(0.2, -1.0, 0, x),
            brightness_contrast_of(0.2, -0.5, 0, x)
        );
    }
}

#[test]
fn brightness_contrast_at_rest_changes_nothing() {
    for x in [0.0, 0.13, 0.5, 0.77, 1.0] {
        // The curves leave the level exactly; the line moves it to mid grey
        // and back, which may round it by a bit.
        assert_eq!(brightness_contrast_of(0.0, 0.0, 0, x), x);
        assert!((brightness_contrast_of(0.0, 0.0, 1, x) - x).abs() < 1e-7);
    }
}

#[test]
fn legacy_brightness_contrast_is_a_line_about_mid_grey_clipped_at_the_end() {
    // Brightness adds its share of the full range.
    assert!((brightness_contrast_of(0.5, 0.0, 1, 0.2) - 0.7).abs() < 1e-6);
    // Contrast 0.5 doubles the distance from mid grey, -0.5 halves it.
    assert!((brightness_contrast_of(0.0, 0.5, 1, 0.6) - 0.7).abs() < 1e-6);
    assert!((brightness_contrast_of(0.0, -0.5, 1, 0.9) - 0.7).abs() < 1e-6);
    // Nothing is clipped between the two: 0.9 + 0.5 is 0.9 from grey,
    // halved.
    assert!((brightness_contrast_of(0.5, -0.5, 1, 0.9) - 0.95).abs() < 1e-6);
    // Contrast 1 is a threshold at mid grey, -1 flat grey.
    assert_eq!(brightness_contrast_of(0.0, 1.0, 1, 0.5), 1.0);
    assert_eq!(brightness_contrast_of(0.0, 1.0, 1, 0.49), 0.0);
    assert_eq!(brightness_contrast_of(0.1, 1.0, 1, 0.41), 1.0);
    assert_eq!(brightness_contrast_of(0.3, -1.0, 1, 0.9), 0.5);
    // The result is clipped.
    assert_eq!(brightness_contrast_of(1.0, 0.0, 1, 0.3), 1.0);
}
