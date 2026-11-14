//! [`Judge`]: how well candidate font files set the text a project stored.

use std::cmp::Ordering;

use motion_format::Result;
use serde::Serialize;

/// A font file a [`Judge`] is asked about.
#[derive(Debug, Clone, Copy)]
pub struct FontFile<'a> {
    /// A name for messages, such as the file's origin.
    pub name: &'a str,
    /// Its bytes.
    pub bytes: &'a [u8],
}

/// Holds font files against evidence of which files a project was made with,
/// such as the lines and glyphs it stored for its text. The resolver asks it
/// about each candidate release and keeps the one it scores best.
pub trait Judge {
    /// How each piece of evidence is set by `files`, one entry per piece.
    ///
    /// # Errors
    /// A diagnostic that ends the resolution; evidence the files cannot set,
    /// such as text in a font they lack, is a [`TextScore`] with
    /// `set: None`, not an error.
    fn judge(&mut self, files: &[FontFile<'_>]) -> Result<Vec<TextScore>>;
}

/// How one stored text is set by candidate files.
#[derive(Debug, Clone, PartialEq)]
pub struct TextScore {
    /// The PostScript names the text asks for.
    pub fonts: Vec<String>,
    /// How its lines compare, or none when the files lack one of its fonts.
    pub set: Option<LinesScore>,
}

/// How a text's lines compare with the stored ones.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct LinesScore {
    /// Stored glyph ids the candidate gives another glyph, or none.
    pub glyphs_differ: usize,
    /// Stored glyph ids compared.
    pub glyphs: usize,
    /// Lines further from the stored ones than the tolerance.
    pub lines_failing: usize,
    /// Lines compared.
    pub lines: usize,
    /// The sum of the lines' distances, in pixels, each capped at a line's
    /// worth so one broken line does not hide the rest.
    pub off: f64,
}

/// The cap on one line's distance in [`LinesScore::off`].
pub const LINE_OFF_CAP: f64 = 1000.0;

/// The score of a candidate: the [`TextScore`]s of the texts in the fonts
/// being chosen, added up. Fewer texts left unset is better, then fewer
/// glyphs that differ, then fewer failing lines, then a smaller distance.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Score {
    /// Texts the files cannot set.
    pub unset: usize,
    /// Texts scored.
    pub texts: usize,
    /// Their lines, added up.
    pub lines: LinesScore,
}

impl Score {
    /// The score of the texts among `texts` that ask for one of `fonts`.
    pub fn of(texts: &[TextScore], fonts: &[String]) -> Self {
        let mut score = Score::default();
        for text in texts
            .iter()
            .filter(|t| t.fonts.iter().any(|f| fonts.contains(f)))
        {
            score.texts += 1;
            let Some(lines) = text.set else {
                score.unset += 1;
                continue;
            };
            score.lines.glyphs_differ += lines.glyphs_differ;
            score.lines.glyphs += lines.glyphs;
            score.lines.lines_failing += lines.lines_failing;
            score.lines.lines += lines.lines;
            score.lines.off += lines.off;
        }
        score
    }

    /// Whether every text is set, every glyph matches and every line is
    /// within the tolerance: nothing could score better.
    pub fn is_exact(&self) -> bool {
        self.unset == 0 && self.lines.glyphs_differ == 0 && self.lines.lines_failing == 0
    }

    /// Whether every text is set and every stored glyph is set as stored:
    /// the candidate is the version, or one with the same glyphs; widths
    /// tell those apart. False without stored glyphs.
    pub fn holds_glyphs(&self) -> bool {
        self.unset == 0 && self.lines.glyphs > 0 && self.lines.glyphs_differ == 0
    }

    /// How this score compares with `other`: [`Ordering::Less`] is better.
    pub fn rank(&self, other: &Score) -> Ordering {
        let key = |s: &Score| (s.unset, s.lines.glyphs_differ, s.lines.lines_failing);
        key(self)
            .cmp(&key(other))
            .then(self.lines.off.total_cmp(&other.lines.off))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(font: &str, set: Option<(usize, usize, f64)>) -> TextScore {
        TextScore {
            fonts: vec![font.into()],
            set: set.map(|(glyphs_differ, lines_failing, off)| LinesScore {
                glyphs_differ,
                glyphs: 10,
                lines_failing,
                lines: 2,
                off,
            }),
        }
    }

    #[test]
    fn counts_only_the_texts_in_the_fonts_asked_about() {
        let texts = [
            text("A-Bold", Some((1, 1, 2.0))),
            text("B-Bold", None),
            text("A-Light", Some((0, 0, 0.5))),
        ];
        let score = Score::of(&texts, &["A-Bold".into(), "A-Light".into()]);
        assert_eq!((score.texts, score.unset), (2, 0));
        assert_eq!(score.lines.glyphs_differ, 1);
        assert_eq!(score.lines.off, 2.5);
        assert!(Score::of(&texts, &["B-Bold".into()]).unset == 1);
    }

    #[test]
    fn ranks_glyphs_before_distance() {
        let exact_glyphs = Score::of(&[text("A", Some((0, 1, 9.0)))], &["A".into()]);
        let near = Score::of(&[text("A", Some((2, 0, 0.1)))], &["A".into()]);
        let unset = Score::of(&[text("A", None)], &["A".into()]);
        assert_eq!(exact_glyphs.rank(&near), Ordering::Less);
        assert_eq!(near.rank(&unset), Ordering::Less);
        assert!(!exact_glyphs.is_exact());
        assert!(Score::of(&[text("A", Some((0, 0, 0.1)))], &["A".into()]).is_exact());
    }
}
