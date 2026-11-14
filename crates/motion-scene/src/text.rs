//! Text measurement, behind a trait so real shaping can replace the estimate.

use motion_format::shorthand::Font;

/// Measures a line of text in pixels.
pub trait TextMeasure {
    /// Returns `(width, height)` of `text` set in `font`.
    fn measure(&self, text: &str, font: &Font) -> (f64, f64);
}

/// A placeholder until real shaping (M2): 0.55 em per character, 1.2 em per line.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApproxTextMeasure;

impl TextMeasure for ApproxTextMeasure {
    fn measure(&self, text: &str, font: &Font) -> (f64, f64) {
        let chars = text.chars().count() as f64;
        (chars * 0.55 * font.size, 1.2 * font.size)
    }
}

#[cfg(test)]
#[path = "../tests/unit/text.rs"]
mod tests;
