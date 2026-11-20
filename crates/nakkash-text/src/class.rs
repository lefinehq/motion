//! What kind of face a font is, read from its own tables: what the type
//! axis of a generated variant pairs faces by (D78).

use skrifa::instance::Size;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, MetadataProvider};

use crate::font::Face;

/// A face's design class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub enum Class {
    /// Serifed text faces.
    Serif,
    /// Slab serifs.
    Slab,
    /// Faces without serifs.
    Sans,
    /// Every glyph one width.
    Mono,
    /// Handwriting and script.
    Script,
    /// Display and decorative faces.
    Display,
}

/// A face's class and the measures that decide how much room its text
/// takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Its class.
    pub class: Class,
    /// The average advance, in ems.
    pub width: f64,
    /// The height of a lowercase x, in ems.
    pub x_height: f64,
    /// The width class, 1 (ultra-condensed) to 9 (ultra-expanded).
    pub stretch: u16,
    /// How thick its strokes are, in ems: the mean width of the strokes of
    /// `H`, `O`, `n` and `o`, twice their ink over their outline's length.
    /// What a weight's number does not say from one family to another: a
    /// condensed poster face at 400 draws strokes as thick as a text
    /// face's 800.
    pub stroke: f64,
}

impl Face {
    /// The face's class and measures, from its `OS/2` and `post` tables:
    /// the IBM family class first, then PANOSE, then its names, so a face
    /// that fills neither table in is still placed.
    pub fn metrics(&self) -> Metrics {
        let font = FontRef::from_index(self.data.data.data(), self.data.index).ok();
        let os2 = font.as_ref().and_then(|f| f.os2().ok());
        let em = font
            .as_ref()
            .and_then(|f| f.head().ok())
            .map_or(1000.0, |h| f64::from(h.units_per_em()));
        let fixed = font
            .as_ref()
            .and_then(|f| f.post().ok())
            .is_some_and(|p| p.is_fixed_pitch() != 0);
        let family = os2.as_ref().map_or(0, |o| (o.s_family_class() >> 8) & 0xFF);
        let panose = os2
            .as_ref()
            .map(|o| o.panose_10().to_vec())
            .unwrap_or_default();
        let name = format!("{} {}", self.family, self.postscript_name).to_lowercase();
        let by_name = |words: &[&str]| words.iter().any(|w| name.contains(w));
        let class = if fixed || panose.get(3) == Some(&9) || by_name(&["mono", "code"]) {
            Class::Mono
        } else if family == 10
            || panose.first() == Some(&3)
            || by_name(&["script", "hand", "vibes"])
        {
            Class::Script
        } else if family == 5 || by_name(&["slab"]) {
            Class::Slab
        } else if family == 9 || panose.first() == Some(&4) || by_name(&["display", "poster"]) {
            Class::Display
        } else if family == 8
            || matches!(panose.get(1), Some(11..=15))
            || by_name(&["sans", "grotesk", "gothic"])
        {
            Class::Sans
        } else if matches!(family, 1..=7)
            || matches!(panose.get(1), Some(2..=10))
            || by_name(&["serif"])
        {
            Class::Serif
        } else {
            Class::Sans
        };
        let width = os2
            .as_ref()
            .map_or(0.5, |o| f64::from(o.x_avg_char_width()) / em);
        let x_height = os2
            .as_ref()
            .and_then(|o| o.sx_height())
            .map_or(0.5, |x| f64::from(x) / em);
        let stretch = os2.as_ref().map_or(5, |o| o.us_width_class());
        // A face set very narrow and heavy is for display, whatever its
        // serifs.
        let class = match class {
            Class::Sans | Class::Serif if stretch <= 3 && self.weight >= 600.0 => Class::Display,
            other => other,
        };
        let stroke = font
            .as_ref()
            .map_or(0.0, |f| stroke(f, &self.variations, em));
        Metrics {
            class,
            width,
            x_height,
            stretch,
            stroke,
        }
    }
}

/// The glyphs a face's [`Metrics::stroke`] is measured on: straight and
/// round stems, capital and lowercase.
const STROKE_GLYPHS: [char; 4] = ['H', 'O', 'n', 'o'];

/// The mean stroke width of [`STROKE_GLYPHS`] in `font` at `variations`, in
/// ems; 0 when it draws none of them.
fn stroke(font: &FontRef, variations: &[(skrifa::Tag, f32)], em: f64) -> f64 {
    let location = font.axes().location(variations.iter().copied());
    let outlines = font.outline_glyphs();
    let charmap = font.charmap();
    let mut ink = Ink::default();
    for c in STROKE_GLYPHS {
        let Some(glyph) = charmap.map(c).and_then(|id| outlines.get(id)) else {
            continue;
        };
        let mut one = Ink::default();
        if glyph
            .draw(
                DrawSettings::unhinted(Size::unscaled(), &location),
                &mut one,
            )
            .is_ok()
        {
            ink.area += one.area.abs();
            ink.length += one.length;
        }
    }
    match ink.length > 0.0 {
        true => 2.0 * ink.area / ink.length / em,
        false => 0.0,
    }
}

/// How many straight pieces a curve is measured as.
const CURVE_PIECES: u16 = 8;

/// A pen that sums what an outline encloses, signed so that holes count
/// against it, and how long it runs.
#[derive(Default)]
struct Ink {
    /// Twice the signed area enclosed so far, halved at the end of each
    /// piece.
    area: f64,
    /// The outline's length so far.
    length: f64,
    /// The pen's point.
    at: (f64, f64),
    /// Where the open contour started.
    start: (f64, f64),
}

impl Ink {
    /// A straight piece from the pen's point to `to`.
    fn to(&mut self, to: (f64, f64)) {
        let (x0, y0) = self.at;
        self.area += (x0 * to.1 - to.0 * y0) / 2.0;
        self.length += (to.0 - x0).hypot(to.1 - y0);
        self.at = to;
    }

    /// A curve through `point(t)` for `t` from 0 to 1, as straight pieces.
    fn curve(&mut self, point: impl Fn(f64) -> (f64, f64)) {
        for i in 1..=CURVE_PIECES {
            self.to(point(f64::from(i) / f64::from(CURVE_PIECES)));
        }
    }
}

impl OutlinePen for Ink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.at = (f64::from(x), f64::from(y));
        self.start = self.at;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.to((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (
            self.at,
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(x), f64::from(y)),
        );
        self.curve(|t| {
            let u = 1.0 - t;
            (
                u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
            )
        });
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let p0 = self.at;
        let (p1, p2, p3) = (
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
        self.curve(|t| {
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            )
        });
    }

    fn close(&mut self) {
        if self.at != self.start {
            self.to(self.start);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/class.rs"]
mod tests;
