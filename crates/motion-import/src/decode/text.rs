//! Text documents: the body of a `LIST:btdk`, read into [`TextDocument`]s
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listbtdk)).
//!
//! The body is parsed into a [`Cos`] tree, then the records below are read
//! out of it with serde. Each record is the table of the keys it uses: a
//! field's `rename` is its key in the spec. Keys a record does not name are
//! left in the tree.

use std::ops::Range;

use motion_format::{Diagnostic, Kind, Result};
use serde::Deserialize;

use crate::cos::{self, Cos, de::read};
use crate::model::{
    CharacterStyle, Color, ComposedLine, ComposedSegment, ParagraphStyle, Run, TextDocument,
    TextFrame,
};

/// The paragraph break the file stores, and the one the model uses.
const BREAK: (char, char) = ('\r', '\n');

/// Reads every document of a `LIST:btdk` body, in keyframe order.
///
/// The body is at `path`, which errors start with; a key path inside the
/// body follows it as the spec writes paths (`LIST:btdk/1/1/0`).
///
/// # Errors
/// A diagnostic when the body is not valid object syntax, does not hold the
/// records a text document needs, or a style names a font the table lacks.
pub(super) fn documents(body: &[u8], path: &str) -> Result<Vec<TextDocument>> {
    let inside = |diagnostic: Diagnostic, keys: &str| {
        let after = diagnostic.path.clone().unwrap_or_default();
        diagnostic.at(format!("{path}{keys}{after}"))
    };
    let mut cos = cos::parse(body).map_err(|d| inside(d, ""))?;
    INHERITANCE
        .iter()
        .for_each(|inheritance| inheritance.resolve(&mut cos));
    let body: Body = read(&cos).map_err(|d| inside(d, ""))?;
    let fonts: Vec<String> = body
        .resources
        .fonts
        .value
        .into_iter()
        .map(|font| font.value.info.postscript_name)
        .collect();
    let frames = body
        .resources
        .frames
        .map(|frames| frames.value)
        .unwrap_or_default();
    body.set
        .documents
        .into_iter()
        .enumerate()
        .map(|(index, document)| {
            document
                .model(&fonts, &frames)
                .map_err(|d| inside(d, &format!("/1/1/{index}")))
        })
        .collect()
}

/// How the runs of one kind inherit their style. A run's payload is
/// `<< /0 << /0 () parent-key index style-key <<style>> >> >>`, and a style
/// sheet's is the same: each stores only the keys that differ from its
/// parent sheet, and the document's default style is below every sheet.
/// Files that store every key are unchanged by the inheritance.
struct Inheritance {
    /// Where the runs are, in a document.
    runs: &'static str,
    /// The payload key of the parent sheet's index.
    parent: &'static str,
    /// The payload key of the style.
    style: &'static str,
    /// Where the sheets are, from the top.
    sheets: &'static str,
    /// Where the document's default style is, from the top.
    default: &'static str,
}

/// The two kinds of run: paragraphs (`/0/5`, sheets at `/0/6/0`, default
/// `/1/3`) and characters (`/0/6`, sheets at `/0/5/0`, default `/1/2`).
const INHERITANCE: [Inheritance; 2] = [
    Inheritance {
        runs: "0/5/0",
        parent: "6",
        style: "5",
        sheets: "0/6/0",
        default: "1/3",
    },
    Inheritance {
        runs: "0/6/0",
        parent: "5",
        style: "6",
        sheets: "0/5/0",
        default: "1/2",
    },
];

impl Inheritance {
    /// Replaces each run's style in every document of `cos` with its whole
    /// inherited style: the default, each ancestor sheet from the root down,
    /// the run's sheet, then the run's own keys.
    fn resolve(&self, cos: &mut Cos) {
        let base = self.base(cos);
        let sheets = self.sheets(cos, &base);
        let Some(documents) = cos.get_mut("1/1") else {
            return;
        };
        let payloads = documents
            .items_mut()
            .iter_mut()
            .filter_map(|document| document.get_mut(self.runs))
            .flat_map(|runs| runs.items_mut().iter_mut())
            .filter_map(|run| run.get_mut("0/0"));
        for payload in payloads {
            let inherited = self
                .parent_of(payload)
                .and_then(|index| sheets.get(index))
                .unwrap_or(&base);
            if let Some(style) = payload.get_mut(self.style) {
                *style = inherited.overlay(style);
            }
        }
    }

    /// The document's default style, or an empty one.
    fn base(&self, cos: &Cos) -> Cos {
        cos.get(self.default)
            .cloned()
            .unwrap_or(Cos::Dict(Vec::new()))
    }

    /// Every sheet's whole style, by index, laid over `base`. A parent chain
    /// that loops or leaves the table stops there.
    fn sheets(&self, cos: &Cos, base: &Cos) -> Vec<Cos> {
        let own: Vec<(Option<usize>, Cos)> = match cos.get(self.sheets) {
            Some(Cos::Array(sheets)) => sheets
                .iter()
                .map(|sheet| {
                    let payload = sheet.get("0");
                    let parent = payload.and_then(|payload| self.parent_of(payload));
                    let style = payload.and_then(|payload| payload.get(self.style));
                    (parent, style.cloned().unwrap_or(Cos::Dict(Vec::new())))
                })
                .collect(),
            _ => Vec::new(),
        };
        (0..own.len())
            .map(|index| {
                // The sheet, then its ancestors, each visited at most once.
                let mut seen = vec![false; own.len()];
                let chain: Vec<&Cos> = std::iter::successors(Some(index), |&at| {
                    seen[at] = true;
                    own[at]
                        .0
                        .filter(|&parent| parent < own.len() && !seen[parent])
                })
                .map(|at| &own[at].1)
                .collect();
                chain
                    .iter()
                    .rev()
                    .fold(base.clone(), |style, own| style.overlay(own))
            })
            .collect()
    }

    /// The parent sheet a payload names.
    fn parent_of(&self, payload: &Cos) -> Option<usize> {
        match payload.get(self.parent) {
            Some(Cos::Integer(index)) => usize::try_from(*index).ok(),
            _ => None,
        }
    }
}

/// `<< /0 value >>`: the one-key dictionary most values are wrapped in.
#[derive(Debug, Deserialize)]
struct Wrapped<T> {
    #[serde(rename = "0")]
    value: T,
}

/// The top level: shared resources and the documents.
#[derive(Debug, Deserialize)]
struct Body {
    /// `/0`.
    #[serde(rename = "0")]
    resources: Resources,
    /// `/1`.
    #[serde(rename = "1")]
    set: DocumentSet,
}

/// `/0`: the resources the documents share.
#[derive(Debug, Deserialize)]
struct Resources {
    /// `/0/1`: the font table.
    #[serde(rename = "1")]
    fonts: Wrapped<Vec<Wrapped<FontObject>>>,
    /// `/0/8`: the text frames.
    #[serde(rename = "8")]
    frames: Option<Wrapped<Vec<Wrapped<Frame>>>>,
}

/// A font table entry: `<< /99 /CoolTypeFont /0 << ... >> >>`.
#[derive(Debug, Deserialize)]
struct FontObject {
    /// `/0`.
    #[serde(rename = "0")]
    info: FontInfo,
}

/// What the table says about a font.
#[derive(Debug, Deserialize)]
struct FontInfo {
    /// `/0`: the PostScript name.
    #[serde(rename = "0")]
    postscript_name: String,
}

/// A text frame.
#[derive(Debug, Deserialize)]
struct Frame {
    /// `/1`: the outline of box text, 16 points as x, y pairs.
    #[serde(rename = "1")]
    outline: Option<Wrapped<Vec<f64>>>,
}

/// `/1`: the documents.
#[derive(Debug, Deserialize)]
struct DocumentSet {
    /// `/1/1`: one document per keyframe, or one.
    #[serde(rename = "1")]
    documents: Vec<DocumentRecord>,
}

/// One document.
#[derive(Debug, Deserialize)]
struct DocumentRecord {
    /// `/0`: the text and its runs.
    #[serde(rename = "0")]
    content: Content,
    /// `/1`: the frame it flows into and its stored layout.
    #[serde(rename = "1")]
    layout: Layout,
}

/// A document's text and runs.
#[derive(Debug, Deserialize)]
struct Content {
    /// `/0`: the text, paragraphs ending in CR.
    #[serde(rename = "0")]
    text: String,
    /// `/5`: paragraph runs.
    #[serde(rename = "5")]
    paragraphs: Wrapped<Vec<RunRecord<Wrapped<ParagraphSheet>>>>,
    /// `/6`: character runs.
    #[serde(rename = "6")]
    characters: Wrapped<Vec<RunRecord<Wrapped<CharacterSheet>>>>,
}

/// A run: `<< /0 payload /1 count >>`.
#[derive(Debug, Deserialize)]
struct RunRecord<P> {
    /// `/0`.
    #[serde(rename = "0")]
    payload: P,
    /// `/1`: its length in UTF-16 code units.
    #[serde(rename = "1")]
    count: usize,
}

/// A paragraph run's payload.
#[derive(Debug, Deserialize)]
struct ParagraphSheet {
    /// `/5`.
    #[serde(rename = "5")]
    style: ParagraphRecord,
}

/// A character run's payload.
#[derive(Debug, Deserialize)]
struct CharacterSheet {
    /// `/6`.
    #[serde(rename = "6")]
    style: CharacterRecord,
}

/// A paragraph style; the spec's table of paragraph keys.
#[derive(Debug, Deserialize)]
struct ParagraphRecord {
    #[serde(rename = "0")]
    justification: i64,
    #[serde(rename = "1")]
    first_line_indent: f64,
    #[serde(rename = "2")]
    start_indent: f64,
    #[serde(rename = "3")]
    end_indent: f64,
    #[serde(rename = "4")]
    space_before: f64,
    #[serde(rename = "5")]
    space_after: f64,
    #[serde(rename = "8")]
    leading_type: i64,
    #[serde(rename = "9")]
    auto_hyphenate: bool,
    #[serde(rename = "29")]
    every_line_composer: bool,
}

/// A character style; the spec's table of character keys.
#[derive(Debug, Deserialize)]
struct CharacterRecord {
    #[serde(rename = "0")]
    font: usize,
    #[serde(rename = "1")]
    size: f64,
    #[serde(rename = "2")]
    faux_bold: bool,
    #[serde(rename = "3")]
    faux_italic: bool,
    #[serde(rename = "4")]
    auto_leading: bool,
    #[serde(rename = "5")]
    leading: f64,
    #[serde(rename = "6")]
    horizontal_scale: f64,
    #[serde(rename = "7")]
    vertical_scale: f64,
    #[serde(rename = "8")]
    tracking: f64,
    #[serde(rename = "9")]
    baseline_shift: f64,
    #[serde(rename = "11")]
    kerning: i64,
    #[serde(rename = "12")]
    caps: i64,
    #[serde(rename = "13")]
    baseline_option: i64,
    #[serde(rename = "18")]
    ligatures: bool,
    #[serde(rename = "36")]
    tsume: f64,
    #[serde(rename = "52")]
    no_break: bool,
    #[serde(rename = "53")]
    fill: Paint,
    #[serde(rename = "54")]
    stroke: Paint,
    #[serde(rename = "56")]
    apply_fill: bool,
    #[serde(rename = "57")]
    apply_stroke: bool,
    #[serde(rename = "58")]
    stroke_over_fill: bool,
    #[serde(rename = "62")]
    line_join: i64,
    #[serde(rename = "63")]
    stroke_width: f64,
}

/// `<< /99 /SimplePaint /0 << /0 1 /1 [ a r g b ] >> >>`.
#[derive(Debug, Deserialize)]
struct Paint {
    #[serde(rename = "0")]
    body: PaintBody,
}

/// A paint's colour.
#[derive(Debug, Deserialize)]
struct PaintBody {
    /// `/1`: alpha, red, green, blue, each 0 to 1.
    #[serde(rename = "1")]
    argb: [f64; 4],
}

impl Paint {
    /// The colour, when `applied`.
    fn color(&self, applied: bool) -> Option<Color> {
        let [a, r, g, b] = self.body.argb;
        applied.then_some(Color { r, g, b, a })
    }
}

/// A document's frame and stored layout.
#[derive(Debug, Deserialize)]
struct Layout {
    /// `/0`: the frame it flows into, by index into `/0/8/0`.
    #[serde(rename = "0", default)]
    frame: Vec<Wrapped<usize>>,
    /// `/2`: the composed-line cache.
    #[serde(rename = "2", default)]
    composed: Vec<Composed>,
}

/// A node of the composed-line cache, by its type name at `/99`.
#[derive(Debug, Deserialize)]
#[serde(tag = "99")]
enum Composed {
    /// The cache, a frame, or a region: only their children matter.
    #[serde(rename = "PC", alias = "F", alias = "R")]
    Group {
        #[serde(rename = "6", default)]
        children: Vec<Composed>,
    },
    /// A line.
    #[serde(rename = "L")]
    Line {
        #[serde(rename = "0")]
        origin: Option<Wrapped<[f64; 2]>>,
        #[serde(rename = "10", default)]
        baseline: f64,
        #[serde(rename = "6", default)]
        children: Vec<Composed>,
    },
    /// A segment of a line.
    #[serde(rename = "S")]
    Segment {
        #[serde(rename = "15")]
        info: SegmentInfo,
        #[serde(rename = "0")]
        origin: Option<Wrapped<[f64; 2]>>,
        #[serde(rename = "6", default)]
        children: Vec<Composed>,
    },
    /// A glyph run.
    #[serde(rename = "G")]
    Glyphs {
        /// `/1`: its extent, `[ left top right bottom ]` (inferred).
        #[serde(rename = "1")]
        extent: Option<[f64; 4]>,
        /// `/5`: its glyph ids (inferred).
        #[serde(rename = "5", default)]
        glyphs: Vec<u32>,
    },
    /// A node type this reader does not use.
    #[serde(other)]
    Other,
}

/// `/S /15`: a segment's counts.
#[derive(Debug, Deserialize)]
struct SegmentInfo {
    /// `/0`: its characters.
    #[serde(rename = "0")]
    characters: usize,
    /// `/7`: its character advances, where stored.
    #[serde(rename = "7")]
    advances: Option<Advances>,
}

/// `/S /15/7`: `<< /7 [ advances ] >>`.
#[derive(Debug, Deserialize)]
struct Advances {
    #[serde(rename = "7")]
    cumulative: Vec<f64>,
}

impl DocumentRecord {
    /// The model of this document, with fonts named from `fonts` and its
    /// frame taken from `frames`.
    fn model(self, fonts: &[String], frames: &[Wrapped<Frame>]) -> Result<TextDocument> {
        let text: String = self.content.text.replace(BREAK.0, &BREAK.1.to_string());
        let text = text
            .strip_suffix(BREAK.1)
            .map(str::to_owned)
            .unwrap_or(text);
        let paragraphs = self.content.paragraphs.value;
        let paragraph_ranges = byte_ranges(&text, paragraphs.iter().map(|run| run.count));
        let paragraphs = paragraphs
            .into_iter()
            .zip(paragraph_ranges)
            .map(|(run, range)| Run {
                range,
                style: run.payload.value.style.model(),
            })
            .collect();
        let characters = self.content.characters.value;
        let character_ranges = byte_ranges(&text, characters.iter().map(|run| run.count));
        let characters = characters
            .into_iter()
            .zip(character_ranges)
            .enumerate()
            .map(|(index, (run, range))| {
                let style = run
                    .payload
                    .value
                    .style
                    .model(fonts)
                    .map_err(|d| d.at(format!("/0/6/0/{index}/0/0/6/0")))?;
                Ok(Run { range, style })
            })
            .collect::<Result<_>>()?;
        let frame_index = self.layout.frame.first().map_or(0, |frame| frame.value);
        let frame = frames
            .get(frame_index)
            .and_then(|frame| frame.value.outline.as_ref())
            .map_or(TextFrame::Point, |outline| text_box(&outline.value));
        let mut lines = Vec::new();
        collect_lines(&self.layout.composed, &mut lines);
        Ok(TextDocument {
            text,
            paragraphs,
            characters,
            frame,
            lines,
        })
    }
}

impl ParagraphRecord {
    /// The paragraph style it describes.
    fn model(&self) -> ParagraphStyle {
        ParagraphStyle {
            justification: self.justification.into(),
            first_line_indent: self.first_line_indent,
            start_indent: self.start_indent,
            end_indent: self.end_indent,
            space_before: self.space_before,
            space_after: self.space_after,
            leading_type: self.leading_type.into(),
            auto_hyphenate: self.auto_hyphenate,
            every_line_composer: self.every_line_composer,
        }
    }
}

impl CharacterRecord {
    /// The character style it describes, its font named from `fonts`.
    ///
    /// # Errors
    /// A diagnostic when the font index is past the end of the table.
    fn model(&self, fonts: &[String]) -> Result<CharacterStyle> {
        let font = fonts.get(self.font).cloned().ok_or_else(|| {
            Diagnostic::new(
                Kind::Invalid,
                format!(
                    "a character style uses font {}, but the font table holds {}",
                    self.font,
                    fonts.len()
                ),
            )
            .expected("a font index into the table at `/0/1/0`")
        })?;
        Ok(CharacterStyle {
            font,
            size: self.size,
            faux_bold: self.faux_bold,
            faux_italic: self.faux_italic,
            auto_leading: self.auto_leading,
            leading: self.leading,
            horizontal_scale: self.horizontal_scale,
            vertical_scale: self.vertical_scale,
            tracking: self.tracking,
            baseline_shift: self.baseline_shift,
            kerning: self.kerning.into(),
            caps: self.caps.into(),
            baseline_option: self.baseline_option.into(),
            ligatures: self.ligatures,
            tsume: self.tsume,
            no_break: self.no_break,
            fill: self.fill.color(self.apply_fill),
            stroke: self.stroke.color(self.apply_stroke),
            stroke_over_fill: self.stroke_over_fill,
            stroke_width: self.stroke_width,
            line_join: self.line_join.into(),
        })
    }
}

/// The byte ranges of `text` that runs of `counts` UTF-16 code units cover,
/// in order. Counts past the end, such as the closing break's, are clipped.
/// A character split by a run boundary belongs to the run it ends in.
fn byte_ranges(text: &str, counts: impl Iterator<Item = usize>) -> Vec<Range<usize>> {
    // Each character's end: in UTF-16 code units and in bytes.
    let mut ends = text
        .char_indices()
        .scan(0, |units, (at, c)| {
            *units += c.len_utf16();
            Some((*units, at + c.len_utf8()))
        })
        .peekable();
    let (mut units, mut start) = (0, 0);
    counts
        .map(|count| {
            units += count;
            let end = std::iter::from_fn(|| ends.next_if(|&(end, _)| end <= units))
                .last()
                .map_or(start, |(_, end)| end);
            let range = start..end;
            start = end;
            range
        })
        .collect()
}

/// A box text's frame, from its outline points: their bounding box.
fn text_box(outline: &[f64]) -> TextFrame {
    let (points, _) = outline.as_chunks::<2>();
    let bounds = points.iter().fold(None, |bounds, &[x, y]| {
        Some(match bounds {
            None => (x, y, x, y),
            Some((left, top, right, bottom)) => (
                f64::min(left, x),
                f64::min(top, y),
                f64::max(right, x),
                f64::max(bottom, y),
            ),
        })
    });
    bounds.map_or(TextFrame::Point, |(left, top, right, bottom)| {
        TextFrame::Box {
            left,
            top,
            width: right - left,
            height: bottom - top,
        }
    })
}

/// Appends the lines below `nodes`, in order.
fn collect_lines(nodes: &[Composed], lines: &mut Vec<ComposedLine>) {
    for node in nodes {
        match node {
            Composed::Group { children } => collect_lines(children, lines),
            Composed::Line {
                origin,
                baseline,
                children,
            } => lines.push(ComposedLine {
                origin: origin.as_ref().map_or([0.0; 2], |origin| origin.value),
                baseline: *baseline,
                segments: segments(children),
            }),
            Composed::Segment { .. } | Composed::Glyphs { .. } | Composed::Other => {}
        }
    }
}

/// The segments among `nodes`.
fn segments(nodes: &[Composed]) -> Vec<ComposedSegment> {
    nodes
        .iter()
        .filter_map(|node| match node {
            Composed::Segment {
                info,
                origin,
                children,
            } => Some(ComposedSegment {
                characters: info.characters,
                origin: origin.as_ref().map_or([0.0; 2], |origin| origin.value),
                advances: info
                    .advances
                    .as_ref()
                    .map(|advances| advances.cumulative.clone())
                    .unwrap_or_default(),
                width: glyph_runs(children)
                    .filter_map(|(extent, _)| extent.map(|[left, _, right, _]| right - left))
                    .sum(),
                glyphs: glyph_runs(children)
                    .flat_map(|(_, glyphs)| glyphs)
                    .copied()
                    .collect(),
            }),
            _ => None,
        })
        .collect()
}

/// The extent and glyph ids of each glyph run among `nodes`, in order.
fn glyph_runs(nodes: &[Composed]) -> impl Iterator<Item = (Option<[f64; 4]>, &[u32])> {
    nodes.iter().filter_map(|node| match node {
        Composed::Glyphs { extent, glyphs } => Some((*extent, glyphs.as_slice())),
        _ => None,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/decode/text.rs"]
mod tests;
