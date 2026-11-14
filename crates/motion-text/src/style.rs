//! What a caller hands the [`Typesetter`](crate::Typesetter): text split into
//! paragraphs, runs of character style over it, a style per paragraph, and
//! the frame it flows into. Lengths are pixels; tracking is in thousandths
//! of an em, as project files store it.

use std::ops::Range;

use motion_format::{Diagnostic, Kind, Result};

use crate::set::FontQuery;

/// The text to lay out and how it is styled.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledText {
    /// The text. Paragraphs are separated by `\n`; U+2028 (line separator)
    /// breaks a line inside a paragraph.
    pub text: String,
    /// The runs of character style, in order, covering the text exactly.
    pub runs: Vec<StyledRun>,
    /// One style per paragraph, in order.
    pub paragraphs: Vec<ParagraphStyle>,
    /// What the text flows into.
    pub frame: TextFrame,
    /// The size of synthesised small capitals, as a fraction of the run's size.
    pub small_caps_scale: f64,
}

/// The size of synthesised small capitals when nothing says otherwise.
pub const SMALL_CAPS_SCALE: f64 = 0.7;

/// The space faux bold adds after each character, in thousandths of an em,
/// as tracking does: the thickened glyphs need room. Measured against a
/// project's stored layout of a 55 px faux bold title, whose three lines it
/// fits within 1.2 px (from 5 to 15 px narrow without it); see the research
/// note on the first sample's fidelity.
pub const FAUX_BOLD_TRACKING: f64 = 30.0;

impl StyledText {
    /// `text` in one character style, every paragraph in the default
    /// paragraph style, as point text.
    pub fn plain(text: impl Into<String>, style: CharacterStyle) -> StyledText {
        let text = text.into();
        let paragraphs = vec![ParagraphStyle::default(); text.split('\n').count()];
        StyledText {
            runs: vec![StyledRun {
                range: 0..text.len(),
                style,
            }],
            text,
            paragraphs,
            frame: TextFrame::Point,
            small_caps_scale: SMALL_CAPS_SCALE,
        }
    }

    /// The paragraphs as byte ranges of [`StyledText::text`], without their `\n`.
    pub fn paragraph_ranges(&self) -> Vec<Range<usize>> {
        let mut start = 0;
        self.text
            .split('\n')
            .map(|p| {
                let range = start..start + p.len();
                start = range.end + 1;
                range
            })
            .collect()
    }

    /// Checks what layout relies on: runs that cover the text in order on
    /// character boundaries, sizes above zero, finite lengths and one style
    /// per paragraph.
    ///
    /// # Errors
    /// A [`Diagnostic`] at the path of the first value that breaks a rule.
    pub fn check(&self) -> Result<()> {
        let paragraphs = self.paragraph_ranges().len();
        if self.paragraphs.len() != paragraphs {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "{} paragraph styles for {paragraphs} paragraphs",
                    self.paragraphs.len()
                ),
            )
            .at("paragraphs")
            .expected("one paragraph style per `\\n`-separated paragraph"));
        }
        let mut end = 0;
        for (i, run) in self.runs.iter().enumerate() {
            let at = |d: Diagnostic| d.under(format!("runs[{i}]"));
            if run.range.start != end || run.range.end <= run.range.start {
                return Err(at(Diagnostic::new(
                    Kind::Invalid,
                    format!(
                        "run {}..{} does not follow on from {end}",
                        run.range.start, run.range.end
                    ),
                )
                .at("range")
                .expected(
                    "runs in order that cover the text, each at least one byte long",
                )));
            }
            if !self.text.is_char_boundary(run.range.start)
                || !self.text.is_char_boundary(run.range.end)
            {
                return Err(at(Diagnostic::new(
                    Kind::Invalid,
                    format!(
                        "run {}..{} splits a character",
                        run.range.start, run.range.end
                    ),
                )
                .at("range")
                .expected("byte ranges on character boundaries")));
            }
            run.style.check().map_err(at)?;
            end = run.range.end;
        }
        if end != self.text.len() {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("the runs end at {end}, the text at {}", self.text.len()),
            )
            .at("runs")
            .expected("runs that cover the whole text"));
        }
        for (i, paragraph) in self.paragraphs.iter().enumerate() {
            paragraph
                .check()
                .map_err(|d| d.under(format!("paragraphs[{i}]")))?;
        }
        if let TextFrame::Box { width, height, .. } = self.frame
            && !(width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite())
        {
            return Err(positive("frame", "a box `width` and `height` above zero"));
        }
        finite(self.small_caps_scale, "small_caps_scale")?;
        Ok(())
    }
}

/// A stretch of [`StyledText::text`] in one character style.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledRun {
    /// The bytes it covers.
    pub range: Range<usize>,
    /// Its style.
    pub style: CharacterStyle,
}

/// The style of a run of characters.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterStyle {
    /// The font it is set in.
    pub font: FontQuery,
    /// The size, in pixels.
    pub size: f64,
    /// Extra space after each character, in thousandths of an em.
    pub tracking: f64,
    /// The distance from the previous baseline.
    pub leading: Leading,
    /// How far the baseline is raised, in pixels.
    pub baseline_shift: f64,
    /// Capitals.
    pub caps: Caps,
    /// Whether a bold is synthesised: layout adds [`FAUX_BOLD_TRACKING`]
    /// after each character, and the renderer thickens the glyphs.
    pub faux_bold: bool,
    /// Whether an italic is synthesised; recorded for the renderer.
    pub faux_italic: bool,
}

impl CharacterStyle {
    /// `font` at `size` pixels, with no tracking, automatic leading, no
    /// baseline shift and the text's own capitals.
    pub fn new(font: FontQuery, size: f64) -> CharacterStyle {
        CharacterStyle {
            font,
            size,
            tracking: 0.0,
            leading: Leading::Auto,
            baseline_shift: 0.0,
            caps: Caps::Normal,
            faux_bold: false,
            faux_italic: false,
        }
    }

    fn check(&self) -> Result<()> {
        if !(self.size > 0.0 && self.size.is_finite()) {
            return Err(positive("style.size", "a size above zero"));
        }
        finite(self.tracking, "style.tracking")?;
        finite(self.baseline_shift, "style.baseline_shift")?;
        if let Leading::Px(px) = self.leading {
            finite(px, "style.leading")?;
        }
        Ok(())
    }
}

/// The distance from one baseline to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leading {
    /// The paragraph's automatic leading times the run's size.
    Auto,
    /// A fixed distance, in pixels.
    Px(f64),
}

/// Capitals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caps {
    /// As typed.
    Normal,
    /// Lower-case letters become small capitals.
    Small,
    /// Every letter becomes a capital.
    All,
    /// Every letter becomes a small capital.
    AllSmall,
}

/// The style of a paragraph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParagraphStyle {
    /// How its lines align.
    pub justification: Justification,
    /// The first line's extra indent, in pixels.
    pub first_line_indent: f64,
    /// The indent at the start of every line.
    pub start_indent: f64,
    /// The indent at the end of every line (box text).
    pub end_indent: f64,
    /// Space above the paragraph, after the first.
    pub space_before: f64,
    /// Space below the paragraph, before the next.
    pub space_after: f64,
    /// Automatic leading as a multiple of the size.
    pub auto_leading: f64,
}

/// Automatic leading when nothing says otherwise: 120% of the size.
pub const AUTO_LEADING: f64 = 1.2;

impl Default for ParagraphStyle {
    fn default() -> ParagraphStyle {
        ParagraphStyle {
            justification: Justification::Left,
            first_line_indent: 0.0,
            start_indent: 0.0,
            end_indent: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            auto_leading: AUTO_LEADING,
        }
    }
}

impl ParagraphStyle {
    fn check(&self) -> Result<()> {
        let lengths = [
            (self.first_line_indent, "first_line_indent"),
            (self.start_indent, "start_indent"),
            (self.end_indent, "end_indent"),
            (self.space_before, "space_before"),
            (self.space_after, "space_after"),
            (self.auto_leading, "auto_leading"),
        ];
        lengths.iter().try_for_each(|&(v, name)| finite(v, name))
    }
}

/// How a paragraph's lines align.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justification {
    /// Flush left.
    Left,
    /// Flush right.
    Right,
    /// Centred.
    Center,
    /// Justified, the last line flush left.
    JustifyLastLeft,
    /// Justified, the last line flush right.
    JustifyLastRight,
    /// Justified, the last line centred.
    JustifyLastCenter,
    /// Justified, the last line too.
    JustifyAll,
}

/// What the text flows into.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextFrame {
    /// Point text: each paragraph is one line, aligned about the origin.
    Point,
    /// Box text: lines wrap inside a box, in the layout's pixels.
    Box {
        /// The left edge.
        left: f64,
        /// The top edge.
        top: f64,
        /// The width.
        width: f64,
        /// The height.
        height: f64,
    },
}

fn finite(value: f64, at: &str) -> Result<()> {
    if value.is_finite() {
        return Ok(());
    }
    Err(Diagnostic::new(
        Kind::OutOfRange,
        format!("`{value}` is not a finite number"),
    )
    .at(at)
    .expected("a finite number"))
}

fn positive(at: &str, expected: &'static str) -> Diagnostic {
    Diagnostic::new(Kind::OutOfRange, "must be above zero")
        .at(at)
        .expected(expected)
}

#[cfg(test)]
#[path = "../tests/unit/style.rs"]
mod tests;
