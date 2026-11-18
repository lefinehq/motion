//! Colour effects: each changes every pixel on its own, on its straight
//! (not premultiplied) colour, and keeps its alpha unless it works on the
//! alpha channel.
//!
//! The formulas follow the usual definitions of these corrections. Where a
//! project's effect has more than one reading, the one chosen is said at
//! the function.

use nakkash_format::spline::Spline;
use vello_cpu::kurbo::{Affine, Point, Rect};

use crate::effect::{Curve, ParamValue, Space};
use crate::par;
use crate::pixels::Pixels;

/// Rec. 601 luma weights, which tints and colourising read lightness by.
pub const LUMA: [f32; 3] = [0.299, 0.587, 0.114];

/// Runs `change` on the straight RGBA of every pixel that is not
/// transparent, and premultiplies the result again.
fn each(pixels: &mut Pixels, change: impl Fn([f32; 4]) -> [f32; 4] + Sync) {
    par::each(pixels.data_mut(), |_, pixel| {
        let alpha = pixel[3];
        if alpha <= 0.0 {
            return;
        }
        let straight = [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha, alpha];
        let [r, g, b, a] = change(straight).map(|c| c.clamp(0.0, 1.0));
        *pixel = [r * a, g * a, b * a, a];
    });
}

/// The number in `values[i]`.
pub(crate) fn number(values: &[ParamValue], i: usize) -> f32 {
    match values[i] {
        ParamValue::Number(n) => n,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// The choice in `values[i]`.
pub(crate) fn choice(values: &[ParamValue], i: usize) -> usize {
    match values[i] {
        ParamValue::Choice(c) => usize::from(c),
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// The colour in `values[i]`, as straight RGB.
pub(crate) fn rgb(values: &[ParamValue], i: usize) -> [f32; 3] {
    match values[i] {
        ParamValue::Color(c) => [c.r, c.g, c.b],
        _ => unreachable!("`Effect::new` checks the parameter types"),
    }
}

/// Which of a pixel's four channels a `channel` choice changes: `rgb`
/// (0) the three colours, then red, green, blue and alpha.
fn channels(choice: usize) -> &'static [usize] {
    match choice {
        0 => &[0, 1, 2],
        1 => &[0],
        2 => &[1],
        3 => &[2],
        _ => &[3],
    }
}

/// Which pixels' centres lie in a layer's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxTest {
    /// From pixels of the grid to the layer's own coordinates.
    pub back: Affine,
    /// The box, in the layer's own coordinates.
    pub bounds: Rect,
}

impl BoxTest {
    /// The centre of the pixel at column `x`, row `y`, in the layer's own
    /// coordinates.
    pub fn centre(&self, x: usize, y: usize) -> Point {
        self.back * Point::new(x as f64 + 0.5, y as f64 + 0.5)
    }

    /// Whether the centre of the pixel at column `x`, row `y` lies in the
    /// box.
    pub fn contains(&self, x: usize, y: usize) -> bool {
        self.bounds.contains(self.centre(x, y))
    }
}

/// `fill`'s plan on pixels: the colour, at its alpha times the effect's
/// opacity, laid "source-atop" each pixel, or "xor" each pixel of the
/// layer's box when inverted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    /// The colour, straight red, green and blue.
    pub rgb: [f32; 3],
    /// How much of it replaces the content: its alpha times the opacity.
    pub share: f32,
    /// Whether it fills where the content is not, rather than the content.
    pub inverted: bool,
    /// Inverted, the layer's box, where it fills; `None` for every pixel,
    /// and when not inverted.
    pub inside: Option<BoxTest>,
}

/// [`Fill`] from `fill`'s values in `space`.
pub(crate) fn fill_plan(values: &[ParamValue], space: &Space) -> Fill {
    let [
        ParamValue::Color(color),
        ParamValue::Number(opacity),
        ParamValue::Choice(invert),
    ] = values
    else {
        unreachable!("`Effect::new` checks the fill parameters");
    };
    let inverted = *invert != 0;
    Fill {
        rgb: [color.r, color.g, color.b],
        share: color.a * opacity,
        inverted,
        inside: match inverted {
            true => space.bounds.map(|bounds| BoxTest {
                back: space.transform.inverse(),
                bounds,
            }),
            false => None,
        },
    }
}

/// `tint`'s plan: each colour's luma picks a colour between `black` and
/// `white`, which replaces it by `amount`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    /// The colour luma 0 picks, straight RGB.
    pub black: [f32; 3],
    /// The colour luma 1 picks, straight RGB.
    pub white: [f32; 3],
    /// How much of the picked colour replaces the pixel's, 0 to 1.
    pub amount: f32,
}

/// [`Tint`] from `tint`'s values.
pub(crate) fn tint_plan(values: &[ParamValue], _: &Space) -> Tint {
    Tint {
        black: rgb(values, 0),
        white: rgb(values, 1),
        amount: number(values, 2),
    }
}

/// `tint`, see [`Tint`].
pub(crate) fn tint(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Tint {
        black,
        white,
        amount,
    } = tint_plan(values, space);
    each(pixels, |[r, g, b, a]| {
        let luma = r * LUMA[0] + g * LUMA[1] + b * LUMA[2];
        let tinted: [f32; 3] = std::array::from_fn(|c| black[c] + (white[c] - black[c]) * luma);
        [
            r + (tinted[0] - r) * amount,
            g + (tinted[1] - g) * amount,
            b + (tinted[2] - b) * amount,
            a,
        ]
    });
}

/// `hue-saturation`'s plan: in `adjust`, hues turn by `hue` degrees,
/// saturation and lightness move towards their ends by their shares; in
/// `colorize`, every colour takes the `colorize-` hue and saturation and
/// keeps its luma, moved by `colorize-lightness`. Lightness at 1 is white
/// and at -1 black. A share moves a value towards 0 when below 0, towards 1
/// when above.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HueSaturation {
    /// Each colour's hue turned, its saturation and lightness moved.
    Adjust {
        /// How far hues turn, in turns.
        turn: f32,
        /// How far saturation moves, -1 to 1.
        saturation: f32,
        /// How far lightness moves, -1 to 1.
        lightness: f32,
    },
    /// Every colour given one hue and saturation at its own luma.
    Colorize {
        /// The hue, a turn from 0 to 1.
        hue: f32,
        /// The saturation, 0 to 1.
        saturation: f32,
        /// How far lightness moves, -1 to 1.
        lightness: f32,
    },
}

/// [`HueSaturation`] from `hue-saturation`'s values.
pub(crate) fn hue_saturation_plan(values: &[ParamValue], _: &Space) -> HueSaturation {
    match choice(values, 6) == 1 {
        false => HueSaturation::Adjust {
            turn: number(values, 0) / 360.0,
            saturation: number(values, 1),
            lightness: number(values, 2),
        },
        true => HueSaturation::Colorize {
            hue: (number(values, 3) / 360.0).rem_euclid(1.0),
            saturation: number(values, 4),
            lightness: number(values, 5),
        },
    }
}

/// `hue-saturation`, see [`HueSaturation`].
pub(crate) fn hue_saturation(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let towards = |value: f32, by: f32| match by < 0.0 {
        true => value * (1.0 + by),
        false => value + (1.0 - value) * by,
    };
    let lighten = |[r, g, b]: [f32; 3], by: f32| [r, g, b].map(|c| towards(c, by));
    match hue_saturation_plan(values, space) {
        HueSaturation::Adjust {
            turn,
            saturation,
            lightness,
        } => {
            each(pixels, |[r, g, b, a]| {
                let [h, s, l] = to_hsl([r, g, b]);
                let turned = from_hsl([(h + turn).rem_euclid(1.0), towards(s, saturation), l]);
                let [r, g, b] = lighten(turned, lightness);
                [r, g, b, a]
            });
        }
        HueSaturation::Colorize {
            hue,
            saturation,
            lightness,
        } => {
            each(pixels, |[r, g, b, a]| {
                let luma = r * LUMA[0] + g * LUMA[1] + b * LUMA[2];
                let [r, g, b] = lighten(from_hsl([hue, saturation, luma]), lightness);
                [r, g, b, a]
            });
        }
    }
}

/// `exposure`'s plan: light times 2 to the `exposure`, plus `offset`,
/// raised to 1/`gamma`. In `linear` space the values are decoded from sRGB
/// first and encoded again after, as the light a camera records.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exposure {
    /// What light is multiplied by: 2 to the `exposure`.
    pub scale: f32,
    /// What is added to it after.
    pub offset: f32,
    /// The gamma the result is raised to 1 over.
    pub gamma: f32,
    /// Whether values are decoded from sRGB first and encoded after.
    pub linear: bool,
}

/// [`Exposure`] from `exposure`'s values.
pub(crate) fn exposure_plan(values: &[ParamValue], _: &Space) -> Exposure {
    Exposure {
        scale: number(values, 0).exp2(),
        offset: number(values, 1),
        gamma: number(values, 2),
        linear: choice(values, 3) == 0,
    }
}

/// `exposure`, see [`Exposure`].
pub(crate) fn exposure(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Exposure {
        scale,
        offset,
        gamma,
        linear,
    } = exposure_plan(values, space);
    let change = |c: f32| {
        let light = if linear { decode(c) } else { c };
        let light = (light * scale + offset).max(0.0).powf(1.0 / gamma);
        if linear { encode(light) } else { light }
    };
    each(pixels, |[r, g, b, a]| [change(r), change(g), change(b), a]);
}

/// `levels`' plan: each level of the channels maps from `input_black`–
/// `input_white` to `output_black`–`output_white` through a power of
/// 1/`gamma`, clipped. An input range of no width is a threshold at
/// `input_white`; a gamma not above 0 sends every level to `output_black`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Levels {
    /// The level that maps to `output_black`.
    pub input_black: f32,
    /// The level that maps to `output_white`.
    pub input_white: f32,
    /// The gamma levels are raised to 1 over, between the two.
    pub gamma: f32,
    /// What `input_black` becomes.
    pub output_black: f32,
    /// What `input_white` becomes.
    pub output_white: f32,
    /// The channels changed: 0, 1 and 2 red, green and blue, 3 alpha.
    pub channels: &'static [usize],
}

/// [`Levels`] from `levels`' values.
pub(crate) fn levels_plan(values: &[ParamValue], _: &Space) -> Levels {
    let [input_black, input_white, gamma, output_black, output_white] =
        std::array::from_fn(|i| number(values, i));
    Levels {
        input_black,
        input_white,
        gamma,
        output_black,
        output_white,
        channels: channels(choice(values, 5)),
    }
}

/// `levels`, see [`Levels`].
pub(crate) fn levels(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Levels {
        input_black,
        input_white,
        gamma,
        output_black,
        output_white,
        channels: changed,
    } = levels_plan(values, space);
    let span = input_white - input_black;
    let map = |c: f32| {
        let t = match span.abs() > f32::EPSILON {
            true => ((c - input_black) / span).clamp(0.0, 1.0),
            false => f32::from(u8::from(c >= input_white)),
        };
        let t = if gamma > 0.0 {
            t.powf(1.0 / gamma)
        } else {
            0.0
        };
        output_black + (output_white - output_black) * t
    };
    each(pixels, |mut pixel: [f32; 4]| {
        for &c in changed {
            pixel[c] = map(pixel[c]);
        }
        pixel
    });
}

/// `invert`'s plan: each level of the channels becomes 1 minus itself, and
/// `blend` of the original stays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Invert {
    /// How much of the original stays, 0 to 1.
    pub blend: f32,
    /// The channels changed: 0, 1 and 2 red, green and blue, 3 alpha.
    pub channels: &'static [usize],
}

/// [`Invert`] from `invert`'s values.
pub(crate) fn invert_plan(values: &[ParamValue], _: &Space) -> Invert {
    Invert {
        blend: number(values, 0),
        channels: channels(choice(values, 1)),
    }
}

/// `invert`, see [`Invert`].
pub(crate) fn invert(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let Invert {
        blend,
        channels: changed,
    } = invert_plan(values, space);
    each(pixels, |mut pixel: [f32; 4]| {
        for &c in changed {
            pixel[c] = (1.0 - pixel[c]) * (1.0 - blend) + pixel[c] * blend;
        }
        pixel
    });
}

/// `black-white`'s plan: a colour's grey is its lowest level, plus what
/// its highest has over its middle by the share of its primary (red, green
/// or blue), plus what its middle has over its lowest by the share of the
/// secondary the two make (yellow, cyan or magenta). White stays white and
/// each pure hue turns as light as its share. With `tint`, the grey takes
/// the hue and saturation of the tint colour, mixed in by `tint`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlackWhite {
    /// The shares of reds, yellows, greens, cyans, blues and magentas.
    pub share: [f32; 6],
    /// How much of the tinted grey replaces the grey, 0 to 1.
    pub tint: f32,
    /// The tint colour's hue, a turn from 0 to 1.
    pub hue: f32,
    /// The tint colour's saturation, 0 to 1.
    pub saturation: f32,
}

/// [`BlackWhite`] from `black-white`'s values.
pub(crate) fn black_white_plan(values: &[ParamValue], _: &Space) -> BlackWhite {
    // reds, yellows, greens, cyans, blues, magentas
    let share: [f32; 6] = std::array::from_fn(|i| number(values, i));
    let (tint, [h, s, _]) = (number(values, 6), to_hsl(rgb(values, 7)));
    BlackWhite {
        share,
        tint,
        hue: h,
        saturation: s,
    }
}

/// `black-white`, see [`BlackWhite`].
pub(crate) fn black_white(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let BlackWhite {
        share,
        tint,
        hue: h,
        saturation: s,
    } = black_white_plan(values, space);
    each(pixels, |[r, g, b, a]| {
        let grey = grey(share, [r, g, b]).clamp(0.0, 1.0);
        let tinted = from_hsl([h, s, grey]);
        let [r, g, b] = tinted.map(|c| grey + (c - grey) * tint);
        [r, g, b, a]
    });
}

/// The grey of `rgb` under the six hues' shares.
fn grey(share: [f32; 6], [r, g, b]: [f32; 3]) -> f32 {
    // Each channel's primary share, and the secondary share of each pair.
    const PRIMARY: [usize; 3] = [0, 2, 4];
    let secondary = |i: usize, j: usize| match (i.min(j), i.max(j)) {
        (0, 1) => 1,
        (1, 2) => 3,
        _ => 5,
    };
    let levels = [r, g, b];
    let mut order = [0, 1, 2];
    order.sort_by(|&i, &j| levels[j].total_cmp(&levels[i]));
    let [high, middle, low] = order;
    levels[low]
        + (levels[high] - levels[middle]) * share[PRIMARY[high]]
        + (levels[middle] - levels[low]) * share[secondary(high, middle)]
}

/// `curves`' plan: each colour level goes through its channel's curve,
/// then red, green and blue through the `rgb` curve, and alpha through its
/// own. A curve works as a table of 256 levels, in whole steps of 1/255 as
/// 8-bit tools keep it, read between its steps.
#[derive(Debug, Clone, PartialEq)]
pub struct Curves {
    /// The tables of the `rgb`, `red`, `green`, `blue` and `alpha` curves,
    /// in that order; `None` for a straight line, which changes nothing.
    pub tables: [Option<CurveTable>; 5],
}

impl Curves {
    /// Whether it changes anything.
    pub fn any(&self) -> bool {
        !self.tables.iter().all(Option::is_none)
    }
}

/// [`Curves`] from `curves`' values.
pub(crate) fn curves_plan(values: &[ParamValue], _: &Space) -> Curves {
    let curve = |i: usize| match values[i] {
        ParamValue::Curve(c) => c,
        _ => unreachable!("`Effect::new` checks the parameter types"),
    };
    Curves {
        tables: std::array::from_fn(|i| CurveTable::new(&curve(i))),
    }
}

/// `curves`, see [`Curves`].
pub(crate) fn curves(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let plan = curves_plan(values, space);
    if !plan.any() {
        return;
    }
    let tables = plan.tables;
    let read = |table: &Option<CurveTable>, c: f32| table.as_ref().map_or(c, |t| t.read(c));
    each(pixels, |[r, g, b, a]| {
        let [r, g, b] =
            [(r, 1), (g, 2), (b, 3)].map(|(c, i)| read(&tables[0], read(&tables[i], c)));
        [r, g, b, read(&tables[4], a)]
    });
}

/// A curve as 256 output levels: the level each input of a whole step of
/// 1/255 gives, itself rounded to a whole step.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveTable(pub [f32; 256]);

impl CurveTable {
    /// The table of `curve`, or `None` for the straight line.
    fn new(curve: &Curve) -> Option<CurveTable> {
        let points = curve.points();
        if points.is_empty() {
            return None;
        }
        let points: Vec<[f64; 2]> = points.iter().map(|p| p.map(f64::from)).collect();
        let spline = Spline::new(&points);
        Some(CurveTable(std::array::from_fn(|i| {
            (spline.at(i as f64 / 255.0) * 255.0)
                .round()
                .clamp(0.0, 255.0) as f32
                / 255.0
        })))
    }

    /// The level `c` maps to, read linearly between the table's steps.
    pub fn read(&self, c: f32) -> f32 {
        let x = c.clamp(0.0, 1.0) * 255.0;
        let i = (x.floor() as usize).min(254);
        let t = x - i as f32;
        self.0[i] + (self.0[i + 1] - self.0[i]) * t
    }
}

/// sRGB's transfer function, from encoded values to light.
fn decode(c: f32) -> f32 {
    match c <= 0.040_45 {
        true => c / 12.92,
        false => ((c + 0.055) / 1.055).powf(2.4),
    }
}

/// sRGB's transfer function, from light to encoded values.
fn encode(light: f32) -> f32 {
    match light <= 0.003_130_8 {
        true => light * 12.92,
        false => 1.055 * light.powf(1.0 / 2.4) - 0.055,
    }
}

/// Hue, saturation and lightness of `rgb`, each 0 to 1.
fn to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= f32::EPSILON {
        return [0.0, 0.0, l];
    }
    let s = match l > 0.5 {
        true => d / (2.0 - max - min),
        false => d / (max + min),
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

/// The RGB of a hue, saturation and lightness, each 0 to 1.
fn from_hsl([h, s, l]: [f32; 3]) -> [f32; 3] {
    if s <= 0.0 {
        return [l; 3];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

/// A pixel's straight colour, for tests.
#[cfg(test)]
pub(crate) fn straight(pixel: crate::pixels::Pixel) -> [f32; 4] {
    let a = pixel[3];
    match a > 0.0 {
        true => [pixel[0] / a, pixel[1] / a, pixel[2] / a, a],
        false => [0.0; 4],
    }
}

#[cfg(test)]
#[path = "../tests/unit/colour.rs"]
mod tests;
