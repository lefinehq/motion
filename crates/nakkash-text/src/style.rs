//! What a caller hands the [`Typesetter`](crate::Typesetter): text split into
//! paragraphs, runs of character style over it, a style per paragraph, and
//! the frame it flows into. Lengths are pixels; tracking is in thousandths
//! of an em, as project files store it.

use std::ops::Range;

use nakkash_format::{Diagnostic, Kind, Result};

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
    /// How it is made to fit the size laid out for it, or `None` to set it
    /// as written.
    pub fit: Option<Fit>,
    /// The size of synthesised small capitals, as a fraction of the run's size.
    pub small_caps_scale: f64,
}

/// How a text is made to fit the size laid out for it, so longer words stay
/// inside it ([D77]). A text never grows: one that fits is set as written.
///
/// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fit {
    /// Point text whose lines are wider than `width` or taller than
    /// `height` scales every size down until they fit, about the vertical
    /// middle of its lines as written.
    Shrink {
        /// The widest its lines may be, in pixels.
        width: f64,
        /// The tallest its lines may be, in pixels.
        height: f64,
    },
    /// Point text breaks its lines at `width`, as box text does, each line
    /// still aligned about the origin; the first baseline stays at the
    /// origin, and text taller than `height` scales down, keeping its top.
    Wrap {
        /// Where its lines break, in pixels.
        width: f64,
        /// The tallest its lines may be, in pixels.
        height: f64,
    },
    /// Box text scales every size down until its wrapped lines fit its box.
    Box,
}

/// The size of synthesised small capitals when nothing says otherwise.
pub const SMALL_CAPS_SCALE: f64 = 0.7;

/// The space faux bold adds after each character, in thousandths of an em,
/// as tracking does: the thickened glyphs need room. Fitted over the
/// corpus's stored layouts: 69 faux bold lines from 25 to 218 px in nine
/// families, whose widths ours exceeded by a median 2.96 thousandths of an
/// em per character at 30 (the first fit, on one 55 px title); a
/// proportional rule fits them with no constant part (0.02795 em less
/// 0.05 px). See the research note on the first sample's fidelity.
pub const FAUX_BOLD_TRACKING: f64 = 27.0;

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
            fit: None,
            small_caps_scale: SMALL_CAPS_SCALE,
        }
    }

    /// The text with every size scaled by `scale`: each run's size, leading
    /// in pixels and baseline shift, and each paragraph's indents and
    /// spacing. Tracking, automatic leading and the small capitals' scale
    /// are fractions of the size, so they follow; the frame stays.
    pub fn scaled(&self, scale: f64) -> StyledText {
        let mut out = self.clone();
        for run in &mut out.runs {
            let style = &mut run.style;
            style.size *= scale;
            style.baseline_shift *= scale;
            if let Leading::Px(px) = &mut style.leading {
                *px *= scale;
            }
        }
        for paragraph in &mut out.paragraphs {
            paragraph.first_line_indent *= scale;
            paragraph.start_indent *= scale;
            paragraph.end_indent *= scale;
            paragraph.space_before *= scale;
            paragraph.space_after *= scale;
        }
        out
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
        if let Some(fit) = self.fit {
            check_fit(fit, self.frame)?;
        }
        finite(self.small_caps_scale, "small_caps_scale")?;
        Ok(())
    }
}

/// Checks that `fit` suits `frame`, and that its size is above zero:
/// `Shrink` and `Wrap` are for point text, `Box` for box text.
fn check_fit(fit: Fit, frame: TextFrame) -> Result<()> {
    let wrong = |what: &str, expected: &'static str| {
        Err(
            Diagnostic::new(Kind::Conflict, format!("{what} cannot fit this way"))
                .at("fit")
                .expected(expected),
        )
    };
    match (fit, frame) {
        (Fit::Shrink { width, height } | Fit::Wrap { width, height }, TextFrame::Point) => {
            if width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite() {
                return Ok(());
            }
            Err(positive("fit", "a `width` and `height` above zero"))
        }
        (Fit::Box, TextFrame::Box { .. }) => Ok(()),
        (Fit::Box, TextFrame::Point) => wrong(
            "point text",
            "`Shrink` or `Wrap` with a size, for point text",
        ),
        (Fit::Shrink { .. } | Fit::Wrap { .. }, TextFrame::Box { .. }) => {
            wrong("box text", "`Box`, which fits box text to its box")
        }
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
    /// How much wider than its size each character is set: its advance,
    /// its tracking and its glyph scale by it. 1 sets it as the font draws it.
    pub horizontal_scale: f64,
    /// How much taller than its size each glyph is drawn; the lines' spacing
    /// does not change.
    pub vertical_scale: f64,
    /// Whether the font's kerning pairs move the characters together or
    /// apart; without, each character advances by its own width only.
    pub kerning: bool,
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
    /// The language of the words, a tag such as `tr`: how capitals are made
    /// and which of the font's local forms are shaped.
    pub lang: Option<String>,
}

impl CharacterStyle {
    /// `font` at `size` pixels, with no tracking, automatic leading, no
    /// baseline shift and the text's own capitals.
    pub fn new(font: FontQuery, size: f64) -> CharacterStyle {
        CharacterStyle {
            font,
            size,
            tracking: 0.0,
            horizontal_scale: 1.0,
            vertical_scale: 1.0,
            kerning: true,
            leading: Leading::Auto,
            baseline_shift: 0.0,
            caps: Caps::Normal,
            faux_bold: false,
            faux_italic: false,
            lang: None,
        }
    }

    fn check(&self) -> Result<()> {
        if !(self.size > 0.0 && self.size.is_finite()) {
            return Err(positive("style.size", "a size above zero"));
        }
        finite(self.tracking, "style.tracking")?;
        if !(self.horizontal_scale > 0.0 && self.horizontal_scale.is_finite()) {
            return Err(positive("style.horizontal_scale", "a scale above zero"));
        }
        if !(self.vertical_scale > 0.0 && self.vertical_scale.is_finite()) {
            return Err(positive("style.vertical_scale", "a scale above zero"));
        }
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
