//! Colours in OKLab and OKLCH, and the change of colour a reference to a
//! colour slot may make ([D77]): `"$accent l-0.021 c+0.004 h+1.6 / 0.5"`
//! is the slot's colour a little darker, a little more saturated, its hue
//! turned 1.6°, at half opacity.
//!
//! OKLab is a perceptual space: equal distances look about equally
//! different, so it decides which colours are one palette entry, and its
//! polar form, OKLCH, says how one colour differs from another in terms a
//! designer uses (lighter, more saturated, warmer), so the difference still
//! reads when the entry changes.
//!
//! [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary

use std::fmt;

use crate::{Diagnostic, Kind, Result};

/// A colour in OKLCH: lightness from 0 to 1, chroma from 0, hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    /// Lightness, 0 (black) to 1 (white).
    pub l: f64,
    /// Chroma, 0 for a grey; about 0.37 at most inside sRGB.
    pub c: f64,
    /// Hue, in degrees from 0 to 360.
    pub h: f64,
}

/// The chroma below which a colour is a grey, whose hue says nothing.
pub const GREY: f64 = 1e-4;

/// An sRGB channel, 0 to 1, as linear light.
fn linear(channel: f64) -> f64 {
    match channel <= 0.04045 {
        true => channel / 12.92,
        false => ((channel + 0.055) / 1.055).powf(2.4),
    }
}

/// A linear-light channel as sRGB, 0 to 1.
fn gamma(channel: f64) -> f64 {
    match channel <= 0.003_130_8 {
        true => channel * 12.92,
        false => 1.055 * channel.powf(1.0 / 2.4) - 0.055,
    }
}

/// `rgb`, sRGB from 0 to 1, in OKLab: `[L, a, b]`.
pub fn oklab(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(linear);
    let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    let [l, m, s] = [l, m, s].map(f64::cbrt);
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// `lab`, a colour in OKLab, as sRGB, each channel clipped to 0 to 1.
pub fn srgb(lab: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = lab;
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let [l, m, s] = [l_, m_, s_].map(|x| x * x * x);
    let r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;
    [r, g, b].map(|c| gamma(c.max(0.0)).clamp(0.0, 1.0))
}

impl Oklch {
    /// `rgb`, sRGB from 0 to 1, in OKLCH.
    pub fn from_srgb(rgb: [f64; 3]) -> Oklch {
        let [l, a, b] = oklab(rgb);
        Oklch {
            l,
            c: a.hypot(b),
            h: b.atan2(a).to_degrees().rem_euclid(360.0),
        }
    }

    /// The colour as sRGB, clipped into it.
    pub fn to_srgb(self) -> [f64; 3] {
        let (sin, cos) = self.h.to_radians().sin_cos();
        srgb([self.l, self.c * cos, self.c * sin])
    }
}

/// How far apart two sRGB colours look: their distance in OKLab, about
/// 0.02 for a difference a viewer just notices.
pub fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let ([l1, a1, b1], [l2, a2, b2]) = (oklab(a), oklab(b));
    ((l1 - l2).powi(2) + (a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt()
}

/// A change of a colour slot's value, which a reference may make: added to
/// its lightness, chroma and hue in OKLCH, and an alpha that replaces its
/// own. [`Shift::NONE`] changes nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shift {
    /// Added to the lightness.
    pub l: f64,
    /// Added to the chroma; the sum stops at 0.
    pub c: f64,
    /// Degrees added to the hue.
    pub h: f64,
    /// The alpha, 0 to 1, in place of the slot's own; none keeps it.
    pub alpha: Option<f64>,
}

/// How a change of colour is written, for errors.
pub const SHIFT_FORM: &str =
    "a reference to a colour slot and a change in OKLCH, e.g. `$accent l-0.02 c+0.004 h+1.5 / 0.5`";

impl Shift {
    /// The change that changes nothing.
    pub const NONE: Shift = Shift {
        l: 0.0,
        c: 0.0,
        h: 0.0,
        alpha: None,
    };

    /// Whether it changes nothing.
    pub fn is_none(&self) -> bool {
        *self == Shift::NONE
    }

    /// `color`, RGBA from 0 to 1, changed.
    pub fn apply(&self, [r, g, b, a]: [f64; 4]) -> [f64; 4] {
        let alpha = self.alpha.unwrap_or(a);
        if self.l == 0.0 && self.c == 0.0 && self.h == 0.0 {
            return [r, g, b, alpha];
        }
        let lch = Oklch::from_srgb([r, g, b]);
        let [r, g, b] = Oklch {
            l: lch.l + self.l,
            c: (lch.c + self.c).max(0.0),
            h: (lch.h + self.h).rem_euclid(360.0),
        }
        .to_srgb();
        [r, g, b, alpha]
    }

    /// The change that turns `from` into `to`, both RGBA from 0 to 1, as
    /// the scene writes it: lightness and chroma to six decimals, the hue
    /// to four, and no turn of hue towards a grey. The alpha is written where the two
    /// differ.
    pub fn between(from: [f64; 4], to: [f64; 4]) -> Shift {
        let (a, b) = (
            Oklch::from_srgb([from[0], from[1], from[2]]),
            Oklch::from_srgb([to[0], to[1], to[2]]),
        );
        let round = |x: f64, places: i32| {
            let scale = 10f64.powi(places);
            let rounded = (x * scale).round() / scale;
            if rounded == 0.0 { 0.0 } else { rounded }
        };
        let same = from[..3] == to[..3];
        let turn = (b.h - a.h + 180.0).rem_euclid(360.0) - 180.0;
        Shift {
            l: if same { 0.0 } else { round(b.l - a.l, 6) },
            c: if same { 0.0 } else { round(b.c - a.c, 6) },
            // A grey's hue is noise, but the same noise each time: the
            // turn from it still gives the colour back.
            h: match same || b.c < GREY {
                true => 0.0,
                false => round(turn, 4),
            },
            alpha: (from[3] != to[3]).then(|| round(to[3], 6)),
        }
    }

    /// Reads a change as a reference writes it after its id: parts
    /// separated by spaces, `l±n`, `c±n` and `h±n` at most once each and in
    /// that order, then `/ a`. None when `text` is not one.
    pub fn parse(text: &str) -> Option<Shift> {
        let (parts, alpha) = match text.split_once('/') {
            Some((parts, alpha)) => {
                let alpha: f64 = alpha.trim().parse().ok()?;
                ((0.0..=1.0).contains(&alpha).then_some(())?);
                (parts, Some(alpha))
            }
            None => (text, None),
        };
        let mut shift = Shift {
            alpha,
            ..Shift::NONE
        };
        let mut next = 0;
        for part in parts.split_whitespace() {
            let (name, number) = part.split_at_checked(1)?;
            let order = ["l", "c", "h"].iter().position(|n| *n == name)?;
            if order < next || !number.starts_with(['+', '-']) {
                return None;
            }
            let number: f64 = number.parse().ok()?;
            if !number.is_finite() {
                return None;
            }
            match order {
                0 => shift.l = number,
                1 => shift.c = number,
                _ => shift.h = number,
            }
            next = order + 1;
        }
        let empty = parts.trim().is_empty() && alpha.is_none();
        (!empty).then_some(shift)
    }

    /// Reads a change, or says why it is not one.
    ///
    /// # Errors
    /// An invalid-value diagnostic naming the form.
    pub fn read(text: &str) -> Result<Shift> {
        Shift::parse(text).ok_or_else(|| {
            Diagnostic::new(Kind::Invalid, format!("`{text}` is not a change of colour"))
                .expected(SHIFT_FORM)
        })
    }
}

impl fmt::Display for Shift {
    /// The change as a reference writes it after its id, the parts that
    /// change nothing left out: `l-0.021 c+0.004 h+1.6 / 0.5`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        for (name, value) in [("l", self.l), ("c", self.c), ("h", self.h)] {
            if value != 0.0 {
                let sign = if value > 0.0 { "+" } else { "-" };
                parts.push(format!("{name}{sign}{}", decimals(value.abs())));
            }
        }
        if let Some(alpha) = self.alpha {
            parts.push(format!("/ {}", decimals(alpha)));
        }
        f.write_str(&parts.join(" "))
    }
}

/// `value` to six decimals, without trailing zeros.
fn decimals(value: f64) -> String {
    let text = format!("{value:.6}");
    match text.trim_end_matches('0').trim_end_matches('.') {
        "-0" => "0".to_owned(),
        trimmed => trimmed.to_owned(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/oklch.rs"]
mod tests;
