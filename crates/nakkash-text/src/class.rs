//! What kind of face a font is, read from its own tables: what the type
//! axis of a generated variant pairs faces by (D78).

use skrifa::FontRef;
use skrifa::raw::TableProvider;

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
        Metrics {
            class,
            width,
            x_height,
            stretch,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/class.rs"]
mod tests;
