//! The face that stands in for a font a set does not have ([D40]): a face of
//! the same family when the set has one, else the built-in Inter, in both
//! cases at the same slant and the nearest weight. Only an import asks for
//! one; a scene that names a font the set lacks is still an error.
//!
//! [D40]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d40-missing-fonts-are-substituted-at-import

use crate::font::{Face, Slant};
use crate::set::FontSet;

/// The family of the built-in faces.
pub(crate) const BUILTIN_FAMILY: &str = "Inter Variable";

/// The style a PostScript name's suffix stands for, as `(suffix, weight,
/// italic)`. A name takes the longest suffix it ends with, in any case;
/// without one it is regular and upright.
pub(crate) const STYLES: &[(&str, f32, bool)] = &[
    ("Thin", 100.0, false),
    ("ThinItalic", 100.0, true),
    ("Hairline", 100.0, false),
    ("HairlineItalic", 100.0, true),
    ("ExtraLight", 200.0, false),
    ("ExtraLightItalic", 200.0, true),
    ("UltraLight", 200.0, false),
    ("UltraLightItalic", 200.0, true),
    ("Light", 300.0, false),
    ("LightItalic", 300.0, true),
    ("Regular", 400.0, false),
    ("Book", 400.0, false),
    ("BookItalic", 400.0, true),
    ("Roman", 400.0, false),
    ("Italic", 400.0, true),
    ("Oblique", 400.0, true),
    ("ItalicMT", 400.0, true),
    ("Medium", 500.0, false),
    ("MediumItalic", 500.0, true),
    ("SemiBold", 600.0, false),
    ("SemiBoldItalic", 600.0, true),
    ("DemiBold", 600.0, false),
    ("DemiBoldItalic", 600.0, true),
    ("Demi", 600.0, false),
    ("DemiItalic", 600.0, true),
    ("Bold", 700.0, false),
    ("BoldItalic", 700.0, true),
    ("BoldOblique", 700.0, true),
    ("BoldMT", 700.0, false),
    ("BoldItalicMT", 700.0, true),
    ("ExtraBold", 800.0, false),
    ("ExtraBoldItalic", 800.0, true),
    ("UltraBold", 800.0, false),
    ("UltraBoldItalic", 800.0, true),
    ("XBold", 800.0, false),
    ("XBoldItalic", 800.0, true),
    ("Heavy", 900.0, false),
    ("HeavyItalic", 900.0, true),
    ("Black", 900.0, false),
    ("BlackItalic", 900.0, true),
];

/// What a PostScript name says about its face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Named<'n> {
    /// The family part: the name before its first `-`, or, without one, the
    /// name without its style suffix (`BebasNeueBold` is `BebasNeue`).
    pub family: &'n str,
    /// The weight its suffix stands for.
    pub weight: f32,
    /// Whether its suffix is an italic one.
    pub italic: bool,
}

impl Named<'_> {
    /// Reads `postscript` through [`STYLES`].
    pub(crate) fn read(postscript: &str) -> Named<'_> {
        let (suffix, weight, italic) = STYLES
            .iter()
            .filter(|(suffix, ..)| ends_with_ignoring_case(postscript, suffix))
            .max_by_key(|(suffix, ..)| suffix.len())
            .copied()
            .unwrap_or(("", 400.0, false));
        let family = match postscript.split_once('-') {
            Some((family, _)) => family,
            None => &postscript[..postscript.len() - suffix.len()],
        };
        Named {
            family,
            weight,
            italic,
        }
    }
}

impl FontSet {
    /// The face to set text in that asks for `postscript`, a face this set
    /// may not have: a face of the same family if there is one, else a
    /// built-in one; of those, the one with the same slant if any, then the
    /// nearest weight, the lighter on a tie. Families are compared without
    /// case, spaces or punctuation, so `OpenSans-Bold` finds `Open Sans`.
    /// `None` when the set has neither.
    pub fn fallback(&self, postscript: &str) -> Option<&Face> {
        let wanted = Named::read(postscript);
        let family = squash(wanted.family);
        let faces = self.faces().iter();
        let mut candidates: Vec<&Face> = faces
            .clone()
            .filter(|face| {
                squash(&face.family) == family
                    || squash(Named::read(&face.postscript_name).family) == family
            })
            .collect();
        if candidates.is_empty() {
            candidates = faces.filter(|f| f.family == BUILTIN_FAMILY).collect();
        }
        closest(candidates, &wanted)
    }
}

/// The face among `candidates` with `wanted`'s slant if any has it, then
/// the nearest weight, the lighter on a tie.
fn closest<'f>(candidates: impl IntoIterator<Item = &'f Face>, wanted: &Named) -> Option<&'f Face> {
    // Weights are compared in tenths, which keeps the order total.
    let tenths = |weight: f32| (weight * 10.0).round() as i32;
    candidates.into_iter().min_by_key(|face| {
        (
            (face.slant != Slant::Normal) != wanted.italic,
            tenths((face.weight - wanted.weight).abs()),
            tenths(face.weight),
        )
    })
}

/// Whether `text` ends with `suffix`, in any case.
fn ends_with_ignoring_case(text: &str, suffix: &str) -> bool {
    text.len() >= suffix.len()
        && text.is_char_boundary(text.len() - suffix.len())
        && text[text.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// `name` in lower case without spaces or punctuation, to compare families.
fn squash(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/fallback.rs"]
mod tests;
