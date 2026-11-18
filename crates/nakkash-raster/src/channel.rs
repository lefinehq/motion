//! Channel effects: effects that rebuild a layer's channels from levels
//! read off pixels, its own or another layer's. `set-matte` takes the
//! layer's alpha from a level of a matte layer; `shift-channels` takes
//! each channel from a level of the layer's own colour.
//!
//! A [`Level`] is one number a pixel gives, 0 to 1: one of its channels, a
//! measure of its colour such as luminance or hue, or a fixed level. The
//! effects here and the displacement map read pixels through it.

use vello_cpu::kurbo::{Affine, Point, Rect, Shape};

use crate::colour::{BoxTest, LUMA, choice};
use crate::distort::Lay;
use crate::effect::{ParamValue, Space};
use crate::pixels::{Pixel, Pixels};

/// One level of a pixel, 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
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
pub enum Reading {
    /// As it is, apart from its alpha: a half-transparent white reads 1.
    Straight,
    /// Laid over black, as the pixels keep it: a half-transparent white
    /// reads 0.5.
    OverBlack,
}

impl Level {
    /// The level of `pixel`, premultiplied, with its colour read as
    /// `reading` says. Alpha, full and off do not depend on the reading.
    pub fn of(self, [r, g, b, a]: Pixel, reading: Reading) -> f32 {
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

/// A layer's box, its own or, for a layer without one, the frame's, as
/// effects that work inside it see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerBox {
    /// From the layer's own coordinates to pixels of the grid.
    pub transform: Affine,
    /// The layer's own box, when it has one.
    pub bounds: Option<Rect>,
}

impl LayerBox {
    /// The box of the layer in `space`.
    pub(crate) fn of(space: &Space) -> LayerBox {
        LayerBox {
            transform: space.transform,
            bounds: space.bounds,
        }
    }

    /// The box on a grid of `width` by `height` pixels, in the layer's own
    /// coordinates: its own, or the bounding box of the grid's.
    pub fn rect(&self, width: u16, height: u16) -> Rect {
        self.bounds.unwrap_or_else(|| {
            let frame = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
            (self.transform.inverse() * frame.to_path(0.1)).bounding_box()
        })
    }

    /// Which pixels of a grid of `width` by `height` lie in the box.
    pub fn test(&self, width: u16, height: u16) -> BoxTest {
        BoxTest {
            back: self.transform.inverse(),
            bounds: self.rect(width, height),
        }
    }
}

/// `set-matte`'s plan: the layer's alpha from a level of the matte layer
/// (its own drawing when it reads none) at the same place, the matte's box
/// stretched over the layer's or centred on it. Inverted, the level is
/// turned into its opposite. Composited, it multiplies the layer's own
/// alpha; otherwise it replaces it inside the layer's box, over the
/// layer's straight colour, black where the layer drew nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetMatte {
    /// The level of the matte read.
    pub level: Level,
    /// Whether the level is turned into its opposite.
    pub inverted: bool,
    /// Whether the matte's box is stretched over the layer's, rather than
    /// centred on it at its own size.
    pub stretch: bool,
    /// Whether the level multiplies the layer's alpha, rather than
    /// replacing it inside the layer's box.
    pub composite: bool,
    /// How the matte's colour is read: laid over black with `premultiply`
    /// on.
    pub reading: Reading,
    /// The layer's box.
    pub area: LayerBox,
    /// The box of the matte layer, when the effect reads one that has a
    /// box; the layer's own box otherwise.
    pub matte_bounds: Option<Rect>,
}

/// [`SetMatte`] from `set-matte`'s values in `space`.
pub(crate) fn set_matte_plan(values: &[ParamValue], space: &Space) -> SetMatte {
    SetMatte {
        level: MATTE_LEVELS[choice(values, 0)],
        inverted: choice(values, 1) == 1,
        stretch: MATTE_LAYS[choice(values, 2)] == Lay::Stretch,
        composite: choice(values, 3) == 0,
        reading: match choice(values, 4) {
            0 => Reading::OverBlack,
            _ => Reading::Straight,
        },
        area: LayerBox::of(space),
        matte_bounds: space.input.as_ref().and_then(|input| input.bounds),
    }
}

/// `set-matte`, see [`SetMatte`]. With `premultiply` on, the matte's
/// colour is read laid over black.
pub(crate) fn set_matte(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let SetMatte {
        level,
        inverted,
        stretch,
        composite,
        reading,
        area,
        matte_bounds,
    } = set_matte_plan(values, space);
    let lay = match stretch {
        true => Lay::Stretch,
        false => Lay::Center,
    };
    let test = area.test(pixels.width(), pixels.height());
    let target = test.bounds;
    let (matte, matte_box) = match &space.input {
        Some(input) => (input.pixels.clone(), matte_bounds.unwrap_or(target)),
        None => (pixels.clone(), target),
    };
    let to: Affine = area.transform;
    let width = usize::from(pixels.width());
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        let at = test.centre(i % width, i / width);
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

/// `shift-channels`' plan: each channel of the layer becomes a level of
/// its straight colour and alpha as they were. Where the layer drew
/// nothing its colour reads black, so only a full alpha raises it, inside
/// the layer's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShiftChannels {
    /// The levels the alpha, red, green and blue take, in that order.
    pub levels: [Level; 4],
    /// The layer's box, which a full alpha fills.
    pub area: LayerBox,
}

impl ShiftChannels {
    /// Whether the alpha is full, so pixels the layer left empty inside its
    /// box change too.
    pub fn fills(&self) -> bool {
        self.levels[0] == Level::Full
    }
}

/// [`ShiftChannels`] from `shift-channels`' values in `space`.
pub(crate) fn shift_channels_plan(values: &[ParamValue], space: &Space) -> ShiftChannels {
    ShiftChannels {
        levels: std::array::from_fn(|i| SHIFT_LEVELS[i][choice(values, i)]),
        area: LayerBox::of(space),
    }
}

/// `shift-channels`, see [`ShiftChannels`].
pub(crate) fn shift_channels(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let plan = shift_channels_plan(values, space);
    let [alpha, red, green, blue] = plan.levels;
    let fills = plan.fills();
    let test = plan.area.test(pixels.width(), pixels.height());
    let width = usize::from(pixels.width());
    for (i, pixel) in pixels.data_mut().iter_mut().enumerate() {
        if pixel[3] <= 0.0 && (!fills || !test.contains(i % width, i / width)) {
            continue;
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
