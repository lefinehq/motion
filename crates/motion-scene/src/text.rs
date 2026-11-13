//! Text measurement: how big a line of text is.
//!
//! Part of step 2 (Compile). The compiler needs a box for every text layer so
//! that later checks can reason about overlap and bounds. Measuring sits behind
//! a trait so a real shaping engine can replace the estimate without touching
//! the compiler.

use motion_format::shorthand::Font;

/// Measures a line of text in pixels.
pub trait TextMeasure {
    /// Returns `(width, height)` of `text` set in `font`.
    fn measure(&self, text: &str, font: &Font) -> (f64, f64);
}

/// A rough estimate: every character is 0.55 em wide and a line is 1.2 em tall.
///
/// This is a placeholder until real text shaping lands (M2). It counts
/// characters, not bytes, so non-Latin scripts and emoji are not inflated.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApproxTextMeasure;

impl TextMeasure for ApproxTextMeasure {
    fn measure(&self, text: &str, font: &Font) -> (f64, f64) {
        let chars = text.chars().count() as f64;
        (chars * 0.55 * font.size, 1.2 * font.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_characters_not_bytes() {
        let f = motion_format::shorthand::Font::parse("Inter 100").unwrap();
        assert_eq!(ApproxTextMeasure.measure("مرحبا", &f).0, 5.0 * 0.55 * 100.0);
        assert_eq!(ApproxTextMeasure.measure("🎬🎬", &f).0, 2.0 * 0.55 * 100.0);
    }

    #[test]
    fn height_is_one_point_two_times_the_size() {
        let f = motion_format::shorthand::Font::parse("Inter 50").unwrap();
        assert_eq!(ApproxTextMeasure.measure("x", &f).1, 60.0);
    }

    #[test]
    fn empty_text_has_zero_width() {
        let f = motion_format::shorthand::Font::parse("Inter 50").unwrap();
        assert_eq!(ApproxTextMeasure.measure("", &f).0, 0.0);
    }
}
