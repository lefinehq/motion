//! Blend modes: how a layer's colours combine with what is below it. The
//! names, their vello equivalents and the formulas the float canvas lays
//! them with are one table, [`BLEND_MODES`].
//!
//! The formulas are those of the W3C compositing and blending module, on
//! colours from 0 to 1: a mode mixes the colour below `b` with the layer's
//! `s`, unpremultiplied, and the mix is laid "source-over"; `add` is the
//! clamped sum of both, alpha included.

use nakkash_format::{Diagnostic, Result};
use vello_cpu::peniko::{self, Compose, Mix};

/// How a layer combines with the layers below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum BlendMode {
    /// The layer covers what is below.
    #[default]
    Normal,
    /// Multiplies colours: never lighter.
    Multiply,
    /// Inverts, multiplies and inverts back: never darker.
    Screen,
    /// Multiply or screen, depending on the colour below.
    Overlay,
    /// The darker of the two colours.
    Darken,
    /// The lighter of the two colours.
    Lighten,
    /// Brightens the colour below to reflect the layer.
    ColorDodge,
    /// Darkens the colour below to reflect the layer.
    ColorBurn,
    /// Multiply or screen, depending on the layer's colour.
    HardLight,
    /// A softer hard light.
    SoftLight,
    /// The absolute difference of the colours.
    Difference,
    /// Like difference, with lower contrast.
    Exclusion,
    /// The layer's hue with the saturation and luminosity below.
    Hue,
    /// The layer's saturation with the hue and luminosity below.
    Saturation,
    /// The layer's hue and saturation with the luminosity below.
    Color,
    /// The layer's luminosity with the hue and saturation below.
    Luminosity,
    /// Adds the colours, clamped: a linear dodge.
    Add,
}

/// One blend mode: its name, the vello mode that draws it, and how the
/// float canvas lays it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlendSpec {
    /// The name scenes use.
    pub name: &'static str,
    /// The mode.
    pub mode: BlendMode,
    vello: peniko::BlendMode,
    laying: Laying,
}

/// An unpremultiplied colour, red, green and blue from 0 to 1.
type Rgb = [f32; 3];

/// How a mode lays a layer's px on the px below, in floats.
#[derive(Debug, Clone, Copy)]
enum Laying {
    /// The colours mixed by the function, from the colour below and the
    /// layer's, laid source-over.
    Over(fn(Rgb, Rgb) -> Rgb),
    /// The sum of both, clamped to 1.
    Plus,
}

impl PartialEq for Laying {
    fn eq(&self, other: &Laying) -> bool {
        match (self, other) {
            (Laying::Over(a), Laying::Over(b)) => std::ptr::fn_addr_eq(*a, *b),
            (Laying::Plus, Laying::Plus) => true,
            _ => false,
        }
    }
}

const fn spec(
    name: &'static str,
    mode: BlendMode,
    (mix, laying): (Mix, Laying),
    compose: Compose,
) -> BlendSpec {
    BlendSpec {
        name,
        mode,
        vello: peniko::BlendMode { mix, compose },
        laying,
    }
}

/// Every blend mode, in the order tools usually list them.
pub const BLEND_MODES: &[BlendSpec] = &[
    spec(
        "normal",
        BlendMode::Normal,
        (Mix::Normal, Laying::Over(normal)),
        Compose::SrcOver,
    ),
    spec(
        "multiply",
        BlendMode::Multiply,
        (Mix::Multiply, Laying::Over(multiply)),
        Compose::SrcOver,
    ),
    spec(
        "screen",
        BlendMode::Screen,
        (Mix::Screen, Laying::Over(screen)),
        Compose::SrcOver,
    ),
    spec(
        "overlay",
        BlendMode::Overlay,
        (Mix::Overlay, Laying::Over(overlay)),
        Compose::SrcOver,
    ),
    spec(
        "darken",
        BlendMode::Darken,
        (Mix::Darken, Laying::Over(darken)),
        Compose::SrcOver,
    ),
    spec(
        "lighten",
        BlendMode::Lighten,
        (Mix::Lighten, Laying::Over(lighten)),
        Compose::SrcOver,
    ),
    spec(
        "color-dodge",
        BlendMode::ColorDodge,
        (Mix::ColorDodge, Laying::Over(color_dodge)),
        Compose::SrcOver,
    ),
    spec(
        "color-burn",
        BlendMode::ColorBurn,
        (Mix::ColorBurn, Laying::Over(color_burn)),
        Compose::SrcOver,
    ),
    spec(
        "hard-light",
        BlendMode::HardLight,
        (Mix::HardLight, Laying::Over(hard_light)),
        Compose::SrcOver,
    ),
    spec(
        "soft-light",
        BlendMode::SoftLight,
        (Mix::SoftLight, Laying::Over(soft_light)),
        Compose::SrcOver,
    ),
    spec(
        "difference",
        BlendMode::Difference,
        (Mix::Difference, Laying::Over(difference)),
        Compose::SrcOver,
    ),
    spec(
        "exclusion",
        BlendMode::Exclusion,
        (Mix::Exclusion, Laying::Over(exclusion)),
        Compose::SrcOver,
    ),
    spec(
        "hue",
        BlendMode::Hue,
        (Mix::Hue, Laying::Over(hue)),
        Compose::SrcOver,
    ),
    spec(
        "saturation",
        BlendMode::Saturation,
        (Mix::Saturation, Laying::Over(saturation)),
        Compose::SrcOver,
    ),
    spec(
        "color",
        BlendMode::Color,
        (Mix::Color, Laying::Over(color)),
        Compose::SrcOver,
    ),
    spec(
        "luminosity",
        BlendMode::Luminosity,
        (Mix::Luminosity, Laying::Over(luminosity)),
        Compose::SrcOver,
    ),
    // Porter-Duff "plus": the sum of both, which the pipeline clamps.
    spec(
        "add",
        BlendMode::Add,
        (Mix::Normal, Laying::Plus),
        Compose::Plus,
    ),
];

impl BlendMode {
    /// The mode called `name` in [`BLEND_MODES`].
    ///
    /// # Errors
    /// An unknown-name diagnostic listing every mode.
    pub fn named(name: &str) -> Result<BlendMode> {
        BLEND_MODES
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.mode)
            .ok_or_else(|| {
                Diagnostic::unknown("blend mode", name, BLEND_MODES.iter().map(|s| s.name))
            })
    }

    /// The mode's row in [`BLEND_MODES`].
    pub fn spec(self) -> &'static BlendSpec {
        BLEND_MODES
            .iter()
            .find(|s| s.mode == self)
            .expect("every blend mode has a row")
    }

    /// The mode's name.
    pub fn name(self) -> &'static str {
        self.spec().name
    }

    pub(crate) fn to_vello(self) -> peniko::BlendMode {
        self.spec().vello
    }

    /// `source`, premultiplied, laid on `below` by this mode, both from 0
    /// to 1: the float canvas's compositing of one px.
    #[inline]
    pub(crate) fn lay(self, below: [f32; 4], source: [f32; 4]) -> [f32; 4] {
        let (source_alpha, below_alpha) = (source[3], below[3]);
        match self.spec().laying {
            Laying::Plus => std::array::from_fn(|c| (below[c] + source[c]).min(1.0)),
            // The common case, without dividing.
            Laying::Over(_) if self == BlendMode::Normal => {
                std::array::from_fn(|c| source[c] + below[c] * (1.0 - source_alpha))
            }
            Laying::Over(mix) => {
                if source_alpha <= 0.0 {
                    return below;
                }
                let unmultiplied = |p: [f32; 4], a: f32| -> Rgb {
                    match a > 0.0 {
                        true => std::array::from_fn(|c| (p[c] / a).clamp(0.0, 1.0)),
                        false => [0.0; 3],
                    }
                };
                let mixed = mix(
                    unmultiplied(below, below_alpha),
                    unmultiplied(source, source_alpha),
                );
                let rgb: Rgb = std::array::from_fn(|c| {
                    source[c] * (1.0 - below_alpha)
                        + below[c] * (1.0 - source_alpha)
                        + source_alpha * below_alpha * mixed[c]
                });
                [
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    source_alpha + below_alpha * (1.0 - source_alpha),
                ]
            }
        }
    }
}

/// Each channel of `b` and `s` mixed by `f`.
fn each(b: Rgb, s: Rgb, f: impl Fn(f32, f32) -> f32) -> Rgb {
    std::array::from_fn(|c| f(b[c], s[c]))
}

fn normal(_: Rgb, s: Rgb) -> Rgb {
    s
}

fn multiply(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, |b, s| b * s)
}

fn screen(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, screen_one)
}

fn screen_one(b: f32, s: f32) -> f32 {
    b + s - b * s
}

fn hard_light_one(b: f32, s: f32) -> f32 {
    match s <= 0.5 {
        true => b * 2.0 * s,
        false => screen_one(b, 2.0 * s - 1.0),
    }
}

fn overlay(b: Rgb, s: Rgb) -> Rgb {
    // Hard light with the two colours swapped.
    each(b, s, |b, s| hard_light_one(s, b))
}

fn darken(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, f32::min)
}

fn lighten(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, f32::max)
}

fn color_dodge(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, |b, s| match (b, s) {
        (b, _) if b <= 0.0 => 0.0,
        (_, s) if s >= 1.0 => 1.0,
        (b, s) => (b / (1.0 - s)).min(1.0),
    })
}

fn color_burn(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, |b, s| match (b, s) {
        (b, _) if b >= 1.0 => 1.0,
        (_, s) if s <= 0.0 => 0.0,
        (b, s) => 1.0 - ((1.0 - b) / s).min(1.0),
    })
}

fn hard_light(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, hard_light_one)
}

fn soft_light(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, |b, s| match s <= 0.5 {
        true => b - (1.0 - 2.0 * s) * b * (1.0 - b),
        false => {
            let d = match b <= 0.25 {
                true => ((16.0 * b - 12.0) * b + 4.0) * b,
                false => b.sqrt(),
            };
            b + (2.0 * s - 1.0) * (d - b)
        }
    })
}

fn difference(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, |b, s| (b - s).abs())
}

fn exclusion(b: Rgb, s: Rgb) -> Rgb {
    each(b, s, |b, s| b + s - 2.0 * b * s)
}

/// The luminosity of a colour, by the module's weights.
fn lum([r, g, b]: Rgb) -> f32 {
    0.3 * r + 0.59 * g + 0.11 * b
}

/// `c` brought within 0 to 1 keeping its luminosity.
fn clip_color(c: Rgb) -> Rgb {
    let l = lum(c);
    let (n, x) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    let mut c = c;
    if n < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    c
}

/// `c` with luminosity `l`.
fn set_lum(c: Rgb, l: f32) -> Rgb {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

/// The saturation of a colour: its widest channel less its narrowest.
fn sat(c: Rgb) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

/// `c` with saturation `s`, its channels keeping their order.
fn set_sat(c: Rgb, s: f32) -> Rgb {
    let (n, x) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    match x > n {
        true => c.map(|v| (v - n) * s / (x - n)),
        false => [0.0; 3],
    }
}

fn hue(b: Rgb, s: Rgb) -> Rgb {
    set_lum(set_sat(s, sat(b)), lum(b))
}

fn saturation(b: Rgb, s: Rgb) -> Rgb {
    set_lum(set_sat(b, sat(s)), lum(b))
}

fn color(b: Rgb, s: Rgb) -> Rgb {
    set_lum(s, lum(b))
}

fn luminosity(b: Rgb, s: Rgb) -> Rgb {
    set_lum(b, lum(s))
}

#[cfg(test)]
#[path = "../tests/unit/blend.rs"]
mod tests;
