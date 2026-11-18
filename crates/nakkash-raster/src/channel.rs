//! Channel effects: effects that rebuild a layer's channels from levels
//! read off pixels, its own or another layer's. `set-matte` takes the
//! layer's alpha from a level of a matte layer; `shift-channels` takes
//! each channel from a level of the layer's own colour.
//!
//! A [`Level`] is one number a pixel gives, 0 to 1: one of its channels, a
//! measure of its colour such as luminance or hue, or a fixed level. The
//! effects here and the displacement map read pixels through it.

use vello_cpu::kurbo::{Affine, Point};

use crate::colour::{LUMA, choice};
use crate::distort::{Lay, layer_box};
use crate::effect::{ParamValue, Space};
use crate::pixels::{Pixel, Pixels};

/// One level of a pixel, 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    /// Its red.
    Red,
    /// Its green.
    Green,
    /// Its blue.
    Blue,
    /// Its alpha.
    Alpha,
    /// The mean of its red, green and blue.
    Intensity,
    /// Its largest colour.
    Value,
    /// Its luma, by Rec. 601 weights.
    Luminance,
    /// Its hue, a turn from red through yellow, green and blue back to red;
    /// 0 for a grey.
    Hue,
    /// The mean of its largest and smallest colour.
    Lightness,
    /// Its saturation, as the hue-lightness-saturation model measures it.
    Saturation,
    /// 1.
    Full,
    /// 0.
    Off,
}

/// How a pixel's colour is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reading {
    /// As it is, apart from its alpha: a half-transparent white reads 1.
    Straight,
    /// Laid over black, as the pixels keep it: a half-transparent white
    /// reads 0.5.
    OverBlack,
}

impl Level {
    /// The level of `pixel`, premultiplied, with its colour read as
    /// `reading` says. Alpha, full and off do not depend on the reading.
    pub(crate) fn of(self, [r, g, b, a]: Pixel, reading: Reading) -> f32 {
        let [r, g, b] = match reading {
            Reading::OverBlack => [r, g, b],
            Reading::Straight if a > 0.0 => [r / a, g / a, b / a],
            Reading::Straight => [0.0; 3],
        };
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let level = match self {
            Level::Red => r,
            Level::Green => g,
            Level::Blue => b,
            Level::Alpha => a,
            Level::Intensity => (r + g + b) / 3.0,
            Level::Value => max,
            Level::Luminance => r * LUMA[0] + g * LUMA[1] + b * LUMA[2],
            Level::Hue => {
                let d = max - min;
                if d <= 0.0 {
                    0.0
                } else if max == r {
                    ((g - b) / d).rem_euclid(6.0) / 6.0
                } else if max == g {
                    ((b - r) / d + 2.0) / 6.0
                } else {
                    ((r - g) / d + 4.0) / 6.0
                }
            }
            Level::Lightness => (max + min) / 2.0,
            Level::Saturation => {
                let (l, d) = ((max + min) / 2.0, max - min);
                if d <= 0.0 {
                    0.0
                } else {
                    d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6)
                }
            }
            Level::Full => 1.0,
            Level::Off => 0.0,
        };
        level.clamp(0.0, 1.0)
    }
}

/// The levels of `set-matte`'s `channel`, in the order of its choices.
const MATTE_LEVELS: [Level; 10] = [
    Level::Alpha,
    Level::Red,
    Level::Green,
    Level::Blue,
    Level::Luminance,
    Level::Hue,
    Level::Lightness,
    Level::Saturation,
    Level::Full,
    Level::Off,
];

/// The placements of `set-matte`'s `behaviour`, in the order of its
/// choices.
const MATTE_LAYS: [Lay; 2] = [Lay::Stretch, Lay::Center];

/// `set-matte`: the layer's alpha from a level of the matte layer (its own
/// drawing when it names none) at the same place, the matte's box
/// stretched over the layer's or centred on it. Inverted, the level is
/// turned into its opposite. Composited, it multiplies the layer's own
/// alpha; otherwise it replaces it inside the layer's box, over the
/// layer's straight colour, black where the layer drew nothing. With
/// `premultiply` on, the matte's colour is read laid over black.
pub(crate) fn set_matte(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let level = MATTE_LEVELS[choice(values, 0)];
    let inverted = choice(values, 1) == 1;
    let lay = MATTE_LAYS[choice(values, 2)];
    let composite = choice(values, 3) == 0;
    let reading = match choice(values, 4) {
        0 => Reading::OverBlack,
        _ => Reading::Straight,
    };
    let target = layer_box(pixels, space);
    let (matte, matte_box) = match &space.input {
        Some(input) => (input.pixels.clone(), input.bounds.unwrap_or(target)),
        None => (pixels.clone(), target),
    };
    let to: Affine = space.transform;
    let back = to.inverse();
    let width = usize::from(pixels.width());
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        let at = back * Point::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
        if !composite && !target.contains(at) {
            continue;
        }
        let [mx, my] = lay.point(target, matte_box, [at.x, at.y]);
        let there = to * Point::new(mx, my);
        let m = level.of(matte.sample(there.x, there.y), reading);
        let m = if inverted { 1.0 - m } else { m };
        *pixel = match composite {
            true => pixel.map(|c| c * m),
            false => {
                let a = pixel[3];
                let straight = |c: f32| if a > 0.0 { c / a } else { 0.0 };
                [
                    straight(pixel[0]) * m,
                    straight(pixel[1]) * m,
                    straight(pixel[2]) * m,
                    m,
                ]
            }
        };
    }
}

/// The levels of `shift-channels`' four settings, `alpha-from`,
/// `red-from`, `green-from` and `blue-from`, in the order of their
/// choices: each lists its own channel first.
const SHIFT_LEVELS: [[Level; 10]; 4] = {
    use Level::{Alpha, Blue, Full, Green, Hue, Lightness, Luminance, Off, Red, Saturation};
    [
        [
            Alpha, Red, Green, Blue, Luminance, Hue, Lightness, Saturation, Full, Off,
        ],
        [
            Red, Alpha, Green, Blue, Luminance, Hue, Lightness, Saturation, Full, Off,
        ],
        [
            Green, Alpha, Red, Blue, Luminance, Hue, Lightness, Saturation, Full, Off,
        ],
        [
            Blue, Alpha, Red, Green, Luminance, Hue, Lightness, Saturation, Full, Off,
        ],
    ]
};

/// `shift-channels`: each channel of the layer becomes a level of its
/// straight colour and alpha as they were: alpha first, then red, green
/// and blue. Where the layer drew nothing its colour reads black, so only
/// a `full` alpha raises it, inside the layer's box.
pub(crate) fn shift_channels(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let [alpha, red, green, blue] = std::array::from_fn(|i| SHIFT_LEVELS[i][choice(values, i)]);
    let fills = alpha == Level::Full;
    let target = layer_box(pixels, space);
    let back = space.transform.inverse();
    let width = usize::from(pixels.width());
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        if pixel[3] <= 0.0 {
            let at = back * Point::new((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
            if !fills || !target.contains(at) {
                continue;
            }
        }
        let was = *pixel;
        let a = alpha.of(was, Reading::Straight);
        let [r, g, b] = [red, green, blue].map(|level| level.of(was, Reading::Straight));
        *pixel = [r * a, g * a, b * a, a];
    }
}

#[cfg(test)]
#[path = "../tests/unit/channel.rs"]
mod tests;
