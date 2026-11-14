//! A text layer's document: its text, the style of every run of it, the
//! frame it flows into, and the layout the project stored for it
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listbtdk)).
//!
//! Ranges are byte ranges into [`TextDocument::text`], converted once from
//! the file's UTF-16 counts, so they slice the text directly. Enumerated
//! styles store their zero-based position in the scripting enumeration.

use std::ops::Range;

use serde::Serialize;

use super::Color;

/// One text document: the value of a Source Text property, or of one of its
/// keyframes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextDocument {
    /// The text. Paragraphs are separated by `\n`; the file's closing
    /// paragraph break is not part of it.
    pub text: String,
    /// The paragraphs and their styles, in order, covering the text.
    pub paragraphs: Vec<Run<ParagraphStyle>>,
    /// The character runs and their styles, in order, covering the text.
    pub characters: Vec<Run<CharacterStyle>>,
    /// What the text flows into.
    pub frame: TextFrame,
    /// The lines as the project last laid them out.
    pub lines: Vec<ComposedLine>,
}

/// A stretch of a document's text with one style.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run<S> {
    /// The bytes of [`TextDocument::text`] it covers.
    pub range: Range<usize>,
    /// Its style.
    pub style: S,
}

/// What a document's text flows into.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextFrame {
    /// Point text: each paragraph is one line, starting at the layer's origin.
    Point,
    /// Box text: lines wrap inside a box, in layer pixels.
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

/// The style of a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ParagraphStyle {
    /// How its lines align.
    pub justification: Justification,
    /// The first line's indent, in pixels.
    pub first_line_indent: f64,
    /// The indent at the start of every line.
    pub start_indent: f64,
    /// The indent at the end of every line.
    pub end_indent: f64,
    /// Space above the paragraph.
    pub space_before: f64,
    /// Space below the paragraph.
    pub space_after: f64,
    /// How leading is measured.
    pub leading_type: LeadingType,
    /// Whether words are hyphenated automatically.
    pub auto_hyphenate: bool,
    /// Whether lines are composed together (else one at a time).
    pub every_line_composer: bool,
}

/// The style of a run of characters.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterStyle {
    /// The font's PostScript name.
    pub font: String,
    /// The size, in pixels.
    pub size: f64,
    /// Whether a bold is synthesised.
    pub faux_bold: bool,
    /// Whether an italic is synthesised.
    pub faux_italic: bool,
    /// Whether the leading follows the size; [`CharacterStyle::leading`] is
    /// then not used.
    pub auto_leading: bool,
    /// The distance between baselines, in pixels.
    pub leading: f64,
    /// Horizontal scale, 1 for none.
    pub horizontal_scale: f64,
    /// Vertical scale, 1 for none.
    pub vertical_scale: f64,
    /// Extra space between characters, in thousandths of an em.
    pub tracking: f64,
    /// How far the baseline is raised, in pixels.
    pub baseline_shift: f64,
    /// How the kerning is chosen.
    pub kerning: Kerning,
    /// Capitals.
    pub caps: Caps,
    /// Superscript or subscript.
    pub baseline_option: BaselineOption,
    /// Whether ligatures are used.
    pub ligatures: bool,
    /// How much space around characters is removed, 0 to 1.
    pub tsume: f64,
    /// Whether the run must not break across lines.
    pub no_break: bool,
    /// The fill colour, when the text is filled.
    pub fill: Option<Color>,
    /// The stroke colour, when the text is stroked.
    pub stroke: Option<Color>,
    /// Whether the stroke is drawn over the fill.
    pub stroke_over_fill: bool,
    /// The stroke's width, in pixels.
    pub stroke_width: f64,
    /// How stroke corners are joined.
    pub line_join: LineJoin,
}

/// One line as the project last laid it out, in layer pixels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComposedLine {
    /// Where the line starts.
    pub origin: [f64; 2],
    /// The baseline's y.
    pub baseline: f64,
    /// Its segments; a line usually has one, even across font changes.
    pub segments: Vec<ComposedSegment>,
}

/// One segment of a [`ComposedLine`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComposedSegment {
    /// How many characters (UTF-16 code units) it holds.
    pub characters: usize,
    /// Where it starts, relative to its line (inferred).
    pub origin: [f64; 2],
    /// Each character's advance, cumulative from the start of the line,
    /// where the project stores them.
    pub advances: Vec<f64>,
    /// The width its glyph runs advance, in pixels (inferred).
    pub width: f64,
    /// The glyph ids of its glyph runs, in order (inferred).
    pub glyphs: Vec<u32>,
}

coded! {
    /// How a paragraph's lines align.
    pub enum Justification: i64 {
        /// Flush left.
        Left = 0,
        /// Flush right.
        Right = 1,
        /// Centred.
        Center = 2,
        /// Justified, the last line flush left.
        JustifyLastLeft = 3,
        /// Justified, the last line flush right.
        JustifyLastRight = 4,
        /// Justified, the last line centred.
        JustifyLastCenter = 5,
        /// Justified, the last line too.
        JustifyAll = 6,
    }
}

coded! {
    /// How leading is measured.
    pub enum LeadingType: i64 {
        /// From baseline to baseline.
        Roman = 0,
        /// From the top of one line to the top of the next.
        Japanese = 1,
    }
}

coded! {
    /// How kerning is chosen.
    pub enum Kerning: i64 {
        /// No automatic kerning.
        None = 0,
        /// The font's kerning pairs.
        Metrics = 1,
        /// Kerning from the glyph shapes.
        Optical = 2,
    }
}

coded! {
    /// Capitals.
    pub enum Caps: i64 {
        /// As typed.
        Normal = 0,
        /// Small capitals.
        Small = 1,
        /// All capitals.
        All = 2,
        /// All small capitals.
        AllSmall = 3,
    }
}

coded! {
    /// Superscript and subscript.
    pub enum BaselineOption: i64 {
        /// On the baseline.
        Normal = 0,
        /// Synthesised superscript.
        Superscript = 1,
        /// Synthesised subscript.
        Subscript = 2,
    }
}

coded! {
    /// How stroke corners are joined.
    pub enum LineJoin: i64 {
        /// Sharp corners.
        Miter = 0,
        /// Rounded corners.
        Round = 1,
        /// Cut-off corners.
        Bevel = 2,
    }
}
