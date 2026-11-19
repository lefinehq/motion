//! Colour arithmetic for the palette axis: sRGB, OKLab and OKLCH, and the
//! WCAG relative luminance that the `contrast` gate measures.
//!
//! A colour moved by a scheme keeps its luminance, so its contrast ratio
//! with every other colour stays what it was ([D78]).
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

/// A straight sRGB colour, each channel 0 to 1, and its alpha.
pub(crate) type Rgba = [f64; 4];

/// Linear light of one sRGB channel.
fn linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The sRGB channel of linear light `c`.
fn encoded(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// The WCAG 2 relative luminance of `rgb`, 0 for black to 1 for white.
pub(crate) fn luminance([r, g, b, _]: Rgba) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// A colour in OKLCH: lightness 0 to 1, chroma, and hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Lch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// `rgb` in OKLCH.
pub(crate) fn to_lch([r, g, b, _]: Rgba) -> Lch {
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lab_l = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let bb = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    Lch {
        l: lab_l,
        c: a.hypot(bb),
        h: bb.atan2(a).to_degrees().rem_euclid(360.0),
    }
}

/// The linear sRGB channels of `lch`, which may lie outside 0 to 1.
fn lch_linear(lch: Lch) -> [f64; 3] {
    let (a, b) = (
        lch.c * lch.h.to_radians().cos(),
        lch.c * lch.h.to_radians().sin(),
    );
    let l = (lch.l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lch.l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lch.l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ]
}

/// Whether `lch` is a colour sRGB can show.
fn in_gamut(lch: Lch) -> bool {
    lch_linear(lch)
        .iter()
        .all(|c| (-1e-6..=1.0 + 1e-6).contains(c))
}

/// `lch` as sRGB with `alpha`, clamped into gamut.
pub(crate) fn from_lch(lch: Lch, alpha: f64) -> Rgba {
    let [r, g, b] = lch_linear(lch).map(|c| encoded(c.clamp(0.0, 1.0)));
    [r, g, b, alpha]
}

/// The luminance of `lch`, clamped into gamut.
fn lch_luminance(lch: Lch) -> f64 {
    let [r, g, b] = lch_linear(lch).map(|c| c.clamp(0.0, 1.0));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// The lightness at which a colour of chroma `c` and hue `h` has
/// luminance `y`, found by bisection: luminance rises with lightness.
fn lightness_for(c: f64, h: f64, y: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if lch_luminance(Lch { l: mid, c, h }) < y {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// The colour of hue `h` and chroma up to `c` whose luminance is `y`: the
/// most chroma, up to `c`, that sRGB can show at that luminance.
pub(crate) fn with_luminance(h: f64, c: f64, y: f64, alpha: f64) -> Rgba {
    let fits = |c: f64| {
        let lch = Lch {
            l: lightness_for(c, h, y),
            c,
            h,
        };
        in_gamut(lch).then_some(lch)
    };
    if let Some(lch) = fits(c) {
        return from_lch(lch, alpha);
    }
    let (mut lo, mut hi) = (0.0, c);
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        match fits(mid) {
            Some(_) => lo = mid,
            None => hi = mid,
        }
    }
    let lch = fits(lo).unwrap_or(Lch {
        l: lightness_for(0.0, h, y),
        c: 0.0,
        h,
    });
    from_lch(lch, alpha)
}

/// `rgb` moved in lightness, hue and chroma kept as far as the gamut lets,
/// until its luminance is `y`.
pub(crate) fn relit(rgb: Rgba, y: f64) -> Rgba {
    let lch = to_lch(rgb);
    with_luminance(lch.h, lch.c, y.clamp(0.0, 1.0), rgb[3])
}

/// How far apart two colours look: the OKLab distance.
pub(crate) fn distance(a: Rgba, b: Rgba) -> f64 {
    let (a, b) = (to_lch(a), to_lch(b));
    let lab = |x: Lch| {
        (
            x.l,
            x.c * x.h.to_radians().cos(),
            x.c * x.h.to_radians().sin(),
        )
    };
    let ((l1, a1, b1), (l2, a2, b2)) = (lab(a), lab(b));
    ((l1 - l2).powi(2) + (a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt()
}

/// Chroma below which a colour reads as a grey, not a hue.
pub(crate) const NEUTRAL: f64 = 0.03;

/// `colour` written as the scene writes colours: `#RRGGBB`, or
/// `#RRGGBBAA` when it is not opaque.
pub(crate) fn hex([r, g, b, a]: Rgba) -> String {
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    match byte(a) {
        255 => format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b)),
        alpha => format!("#{:02X}{:02X}{:02X}{alpha:02X}", byte(r), byte(g), byte(b)),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/vary/colour.rs"]
mod tests;
