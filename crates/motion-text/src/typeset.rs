//! [`Typesetter`]: lays out a [`StyledText`] with Parley, paragraph by
//! paragraph, and places the lines as project files do (see
//! [`crate::layout`] for the coordinates).
//!
//! Parley shapes (HarfRust), breaks lines, indents the first line and aligns
//! box text. Baselines are placed here: each line sits one leading below the
//! previous baseline, so the result does not depend on how a line box splits
//! its leading. Every face is registered under a family name of its own, so a
//! run gets exactly the face it asks for; a run that Parley had to fall back
//! from, or a glyph the font lacks, is an error.

use std::borrow::Cow;
use std::ops::Range;

use kurbo::Size;
use motion_format::{Diagnostic, Kind, Result};
use parley::fontique::{
    Collection, CollectionOptions, FontInfoOverride, FontStyle as Style, FontWeight, SourceCache,
};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontVariation, FontVariations,
    IndentOptions, Layout, LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
};

use crate::caps::{self, Cased};
use crate::font::{Face, Slant};
use crate::layout::{Glyph, GlyphRun, Line, TextLayout};
use crate::set::FontSet;
use crate::style::{
    CharacterStyle, FAUX_BOLD_TRACKING, Justification, Leading, ParagraphStyle, StyledText,
    TextFrame,
};

/// How one justification lays out: Parley's alignment in a box, the last
/// line's, and for point text where the line sits on the origin (0 starts
/// there, 0.5 is centred on it, 1 ends there).
struct JustificationRule {
    justification: Justification,
    in_box: Alignment,
    last_line: Option<Alignment>,
    point_anchor: f64,
}

/// Every justification, as data. A point-text line is its paragraph's last
/// line, so a justified one aligns as its last line would.
const JUSTIFICATIONS: &[JustificationRule] = &[
    JustificationRule {
        justification: Justification::Left,
        in_box: Alignment::Left,
        last_line: None,
        point_anchor: 0.0,
    },
    JustificationRule {
        justification: Justification::Right,
        in_box: Alignment::Right,
        last_line: None,
        point_anchor: 1.0,
    },
    JustificationRule {
        justification: Justification::Center,
        in_box: Alignment::Center,
        last_line: None,
        point_anchor: 0.5,
    },
    JustificationRule {
        justification: Justification::JustifyLastLeft,
        in_box: Alignment::Justify,
        last_line: Some(Alignment::Left),
        point_anchor: 0.0,
    },
    JustificationRule {
        justification: Justification::JustifyLastRight,
        in_box: Alignment::Justify,
        last_line: Some(Alignment::Right),
        point_anchor: 1.0,
    },
    JustificationRule {
        justification: Justification::JustifyLastCenter,
        in_box: Alignment::Justify,
        last_line: Some(Alignment::Center),
        point_anchor: 0.5,
    },
    JustificationRule {
        justification: Justification::JustifyAll,
        in_box: Alignment::Justify,
        last_line: Some(Alignment::Justify),
        point_anchor: 0.0,
    },
];

fn rule(justification: Justification) -> &'static JustificationRule {
    JUSTIFICATIONS
        .iter()
        .find(|r| r.justification == justification)
        .expect("every justification has a rule")
}

/// Lays out text in the faces of one [`FontSet`]. It keeps Parley's caches,
/// so one typesetter per thread serves many layouts.
pub struct Typesetter {
    fonts: FontSet,
    font_cx: FontContext,
    layout_cx: LayoutContext<usize>,
    /// The family each face is registered under, parallel to the set's faces.
    families: Vec<String>,
    /// The axis values each face is set at, parallel to the set's faces.
    variations: Vec<Vec<FontVariation>>,
}

impl std::fmt::Debug for Typesetter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Typesetter")
            .field("faces", &self.fonts.faces().len())
            .finish_non_exhaustive()
    }
}

impl Typesetter {
    /// A typesetter that sets text in `fonts` and nothing else.
    pub fn new(fonts: FontSet) -> Typesetter {
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let families = (0..fonts.faces().len())
            .map(|i| format!("motion-face-{i}"))
            .collect::<Vec<_>>();
        for (face, family) in fonts.faces().iter().zip(&families) {
            collection.register_fonts(
                face.data.data.clone(),
                Some(FontInfoOverride {
                    family_name: Some(family),
                    weight: Some(FontWeight::new(face.weight)),
                    style: Some(font_style(face.slant)),
                    axes: None,
                    width: None,
                }),
            );
        }
        // A named instance is set at its own axis values, not where the font's
        // weight and slant would place it.
        let variations = fonts
            .faces()
            .iter()
            .map(|face| {
                face.variations
                    .iter()
                    .map(|&(tag, value)| {
                        FontVariation::new(parlance::Tag::from_bytes(tag.to_be_bytes()), value)
                    })
                    .collect()
            })
            .collect();
        Typesetter {
            fonts,
            font_cx: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            layout_cx: LayoutContext::new(),
            families,
            variations,
        }
    }

    /// The fonts it sets text in.
    pub fn fonts(&self) -> &FontSet {
        &self.fonts
    }

    /// Lays out `text`.
    ///
    /// # Errors
    /// A [`Diagnostic`] when the text breaks a rule of [`StyledText::check`],
    /// names a font the set does not have, or holds a character its font
    /// has no glyph for.
    pub fn layout(&mut self, text: &StyledText) -> Result<TextLayout> {
        text.check()?;
        if text.runs.is_empty() {
            // Only an empty text has no runs, and it has nothing to lay out.
            return Ok(TextLayout {
                lines: Vec::new(),
                runs: Vec::new(),
            });
        }
        let faces = text
            .runs
            .iter()
            .enumerate()
            .map(|(i, run)| {
                self.fonts
                    .position(&run.style.font)
                    .map_err(|d| d.at(format!("runs[{i}].style.font")))
            })
            .collect::<Result<Vec<usize>>>()?;
        let cased = caps::apply(text);
        let mut out = TextLayout {
            lines: Vec::new(),
            runs: Vec::new(),
        };
        let mut cursor = Cursor::new(text.frame);
        for (index, source) in text.paragraph_ranges().into_iter().enumerate() {
            let paragraph = Paragraph {
                index,
                style: &text.paragraphs[index],
                set: set_range(&cased, &source),
                source,
            };
            self.paragraph(text, &cased, &faces, &paragraph, &mut cursor, &mut out)?;
        }
        Ok(out)
    }

    /// The width and height `text` covers.
    ///
    /// # Errors
    /// As [`Typesetter::layout`].
    pub fn measure(&mut self, text: &StyledText) -> Result<Size> {
        self.layout(text).map(|layout| layout.size())
    }

    /// Lays out one paragraph and appends its lines and runs to `out`.
    fn paragraph(
        &mut self,
        text: &StyledText,
        cased: &Cased,
        faces: &[usize],
        paragraph: &Paragraph,
        cursor: &mut Cursor,
        out: &mut TextLayout,
    ) -> Result<()> {
        let mut layout = self.build(text, cased, faces, paragraph);
        let rule = rule(paragraph.style.justification);
        layout.set_text_indent(
            paragraph.style.first_line_indent as f32,
            IndentOptions::default(),
        );
        // Point text never wraps; its lines are anchored below, so Parley sets them flush left.
        let (available, alignment, last_line) = match text.frame {
            TextFrame::Point => (None, Alignment::Left, None),
            TextFrame::Box { width, .. } => (
                Some(
                    (width - paragraph.style.start_indent - paragraph.style.end_indent).max(0.0)
                        as f32,
                ),
                rule.in_box,
                rule.last_line,
            ),
        };
        layout.set_alignment(
            alignment,
            AlignmentOptions {
                align_when_overflowing: false,
                last_line_alignment: last_line,
            },
        );
        layout.break_all_lines(available);
        cursor.start_paragraph(paragraph.style);
        let reader = Reader {
            text,
            cased,
            faces,
            fonts: &self.fonts,
            base: paragraph.set.start,
        };
        for line in layout.lines() {
            let metrics = line.metrics();
            let ascent = f64::from(metrics.baseline - metrics.content_block_min_coord);
            let descent = f64::from(metrics.content_block_max_coord - metrics.baseline);
            let baseline = cursor.next_baseline(metrics.line_height.into(), ascent);
            // Where Parley starts the line's content, alignment and indent included.
            let content = f64::from(metrics.inline_min_coord + metrics.offset);
            let placed = reader.read(&line, out.lines.len(), baseline)?;
            let line_start = match text.frame {
                TextFrame::Point => {
                    paragraph.style.start_indent + content - rule.point_anchor * placed.width
                }
                TextFrame::Box { left, .. } => left + paragraph.style.start_indent + content,
            };
            let shift = line_start - content;
            out.runs.extend(placed.runs.into_iter().map(|mut run| {
                run.glyphs.iter_mut().for_each(|g| g.x += shift);
                run
            }));
            out.lines.push(Line {
                paragraph: paragraph.index,
                characters: text.text[placed.range.clone()].encode_utf16().count(),
                range: placed.range,
                x: line_start,
                baseline,
                width: placed.width,
                ascent,
                descent,
                advances: placed.advances,
            });
        }
        cursor.end_paragraph(paragraph.style);
        Ok(())
    }

    /// Builds the Parley layout of one paragraph's set text. Each stretch
    /// carries its input run's index as its brush, so a glyph run never spans
    /// two input runs and knows which one it came from.
    fn build(
        &mut self,
        text: &StyledText,
        cased: &Cased,
        faces: &[usize],
        paragraph: &Paragraph,
    ) -> Layout<usize> {
        let set_text = &cased.text[paragraph.set.clone()];
        let styler = Styler {
            text,
            faces,
            families: &self.families,
            variations: &self.variations,
            fonts: &self.fonts,
            paragraph: paragraph.style,
        };
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, set_text, 1.0, false);
        // The run at the paragraph's start styles an empty paragraph's line.
        for property in styler.properties(run_at(text, paragraph.source.start), false) {
            builder.push_default(property);
        }
        for segment in segments(cased, &paragraph.set) {
            let range =
                segment.set.start - paragraph.set.start..segment.set.end - paragraph.set.start;
            for property in styler.properties(segment.run, segment.small) {
                builder.push(property, range.clone());
            }
        }
        builder.build(set_text)
    }
}

/// Turns input runs into Parley styles.
struct Styler<'a> {
    text: &'a StyledText,
    faces: &'a [usize],
    families: &'a [String],
    variations: &'a [Vec<FontVariation>],
    fonts: &'a FontSet,
    paragraph: &'a ParagraphStyle,
}

impl<'a> Styler<'a> {
    /// The Parley style of input run `run`, at small-capital size when `small`.
    fn properties(&self, run: usize, small: bool) -> Vec<StyleProperty<'a, usize>> {
        let style = &self.text.runs[run].style;
        let face_index = self.faces[run];
        let face = &self.fonts.faces()[face_index];
        let size = if small {
            style.size * self.text.small_caps_scale
        } else {
            style.size
        };
        let leading = match style.leading {
            Leading::Auto => LineHeight::FontSizeRelative(self.paragraph.auto_leading as f32),
            Leading::Px(px) => LineHeight::Absolute(px as f32),
        };
        vec![
            StyleProperty::Brush(run),
            StyleProperty::FontFamily(FontFamily::named(&self.families[face_index])),
            StyleProperty::FontWeight(FontWeight::new(face.weight)),
            StyleProperty::FontStyle(font_style(face.slant)),
            StyleProperty::FontVariations(FontVariations::List(Cow::Borrowed(
                &self.variations[face_index],
            ))),
            StyleProperty::FontSize(size as f32),
            // Tracking follows every character, in thousandths of the run's em;
            // faux bold adds its own.
            StyleProperty::LetterSpacing((tracking(style) / 1000.0 * style.size) as f32),
            StyleProperty::LineHeight(leading),
        ]
    }
}

/// The space after each character of `style`, in thousandths of an em.
fn tracking(style: &CharacterStyle) -> f64 {
    match style.faux_bold {
        true => style.tracking + FAUX_BOLD_TRACKING,
        false => style.tracking,
    }
}

fn font_style(slant: Slant) -> Style {
    match slant {
        Slant::Normal => Style::Normal,
        Slant::Italic => Style::Italic,
        Slant::Oblique => Style::Oblique(None),
    }
}

/// One paragraph: its index, style, and its bytes in the source and set texts.
struct Paragraph<'a> {
    index: usize,
    style: &'a ParagraphStyle,
    source: Range<usize>,
    set: Range<usize>,
}

/// The input run that covers source byte `at`; past the end, the last.
fn run_at(text: &StyledText, at: usize) -> usize {
    text.runs
        .iter()
        .position(|r| r.range.contains(&at))
        .unwrap_or(text.runs.len() - 1)
}

/// The set-text bytes of the source paragraph `source`.
fn set_range(cased: &Cased, source: &Range<usize>) -> Range<usize> {
    let inside: Vec<&Range<usize>> = cased
        .spans
        .iter()
        .filter(|s| s.source.start >= source.start && s.source.end <= source.end)
        .map(|s| &s.set)
        .collect();
    match (inside.first(), inside.last()) {
        (Some(first), Some(last)) => first.start..last.end,
        _ => {
            // An empty paragraph: where the next character starts, or the end.
            let at = cased
                .spans
                .iter()
                .find(|s| s.source.start >= source.start)
                .map_or(cased.text.len(), |s| s.set.start);
            at..at
        }
    }
}

/// A stretch of set text in one input run at one size.
struct Segment {
    set: Range<usize>,
    run: usize,
    small: bool,
}

/// The stretches of set text inside `set`.
fn segments(cased: &Cased, set: &Range<usize>) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for span in cased
        .spans
        .iter()
        .filter(|s| s.set.start >= set.start && s.set.end <= set.end)
    {
        match out.last_mut() {
            Some(last)
                if last.set.end == span.set.start
                    && last.run == span.run
                    && last.small == span.small =>
            {
                last.set.end = span.set.end;
            }
            _ => out.push(Segment {
                set: span.set.clone(),
                run: span.run,
                small: span.small,
            }),
        }
    }
    out
}

/// Where the next baseline goes.
struct Cursor {
    frame: TextFrame,
    /// The last baseline, once there is one.
    last: Option<f64>,
    /// Space to add before the next line.
    pending: f64,
}

impl Cursor {
    fn new(frame: TextFrame) -> Cursor {
        Cursor {
            frame,
            last: None,
            pending: 0.0,
        }
    }

    fn start_paragraph(&mut self, style: &ParagraphStyle) {
        // Space before a paragraph counts only between paragraphs.
        if self.last.is_some() {
            self.pending += style.space_before;
        }
    }

    fn end_paragraph(&mut self, style: &ParagraphStyle) {
        self.pending += style.space_after;
    }

    /// The baseline of a line whose leading is `leading` and whose ascent is `ascent`.
    fn next_baseline(&mut self, leading: f64, ascent: f64) -> f64 {
        let baseline = match (self.last, self.frame) {
            (Some(last), _) => last + self.pending + leading,
            (None, TextFrame::Point) => 0.0,
            (None, TextFrame::Box { top, .. }) => top + ascent,
        };
        self.pending = 0.0;
        self.last = Some(baseline);
        baseline
    }
}

/// A line's glyph runs and character advances, x still in Parley's coordinates.
struct Placed {
    range: Range<usize>,
    width: f64,
    advances: Vec<f64>,
    runs: Vec<GlyphRun>,
}

/// One cluster of a line, in visual order.
struct ClusterInfo {
    /// Its source bytes.
    source: Range<usize>,
    /// How many glyphs it owns: a ligature's start owns them all, its
    /// continuations none.
    glyphs: usize,
    /// Its advance, tracking and justification included.
    advance: f64,
    /// Whether it is white space, which a line's width leaves out at its end.
    blank: bool,
    /// Whether it is a control character, which shows nothing.
    control: bool,
    /// Whether it continues a ligature, whose glyphs it then shows too.
    continuation: bool,
}

/// Reads the Parley lines of one paragraph.
struct Reader<'a> {
    text: &'a StyledText,
    cased: &'a Cased,
    faces: &'a [usize],
    fonts: &'a FontSet,
    /// Where the paragraph starts in the set text.
    base: usize,
}

impl Reader<'_> {
    /// Reads one line: its glyphs with their source clusters, checked against
    /// the faces they asked for, and each source character's advance.
    fn read(
        &self,
        line: &parley::Line<'_, usize>,
        line_index: usize,
        baseline: f64,
    ) -> Result<Placed> {
        let clusters: Vec<ClusterInfo> = line
            .runs()
            .flat_map(|run| run.visual_clusters().collect::<Vec<_>>())
            .map(|c| {
                let set = c.text_range();
                let source = self
                    .cased
                    .source_of(self.base + set.start..self.base + set.end);
                let shown = &self.text.text[source.clone()];
                ClusterInfo {
                    glyphs: c.glyphs().count(),
                    advance: c.advance().into(),
                    blank: c.is_space_or_nbsp() || shown.chars().all(char::is_whitespace),
                    control: !shown.is_empty() && shown.chars().all(char::is_control),
                    continuation: c.is_ligature_continuation(),
                    source,
                }
            })
            .collect();
        let runs = self.glyph_runs(line, line_index, baseline, &clusters)?;
        // Each source character's own advance, by its first byte; a cluster's
        // advance is shared evenly by its characters.
        let mut by_char: Vec<(usize, f64)> = Vec::new();
        let mut pen = 0.0;
        let mut width: f64 = 0.0;
        for cluster in &clusters {
            let chars: Vec<usize> = self.text.text[cluster.source.clone()]
                .char_indices()
                .map(|(o, _)| cluster.source.start + o)
                .collect();
            let share = cluster.advance / chars.len().max(1) as f64;
            for at in chars {
                match by_char.iter_mut().find(|(c, _)| *c == at) {
                    Some(entry) => entry.1 += share,
                    None => by_char.push((at, share)),
                }
            }
            pen += cluster.advance;
            if !cluster.blank {
                width = width.max(pen);
            }
        }
        let line_text = line.text_range();
        let range = self.trim_break(
            self.cased
                .source_of(self.base + line_text.start..self.base + line_text.end),
        );
        let mut total = 0.0;
        let advances = self.text.text[range.clone()]
            .char_indices()
            .map(|(o, _)| {
                let at = range.start + o;
                total += by_char
                    .iter()
                    .find(|(c, _)| *c == at)
                    .map_or(0.0, |(_, a)| *a);
                total
            })
            .collect();
        Ok(Placed {
            range,
            width,
            advances,
            runs,
        })
    }

    /// The line's glyph runs, each glyph tagged with the source bytes it shows.
    fn glyph_runs(
        &self,
        line: &parley::Line<'_, usize>,
        line_index: usize,
        baseline: f64,
        clusters: &[ClusterInfo],
    ) -> Result<Vec<GlyphRun>> {
        let mut next = clusters.iter().peekable();
        let mut runs = Vec::new();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let style_run = glyph_run.style().brush;
            let face_index = self.faces[style_run];
            let face = &self.fonts.faces()[face_index];
            let run = glyph_run.run();
            let font = run.font();
            let same_face = font.data.id() == face.data.data.id() && font.index == face.data.index;
            let shift = self.text.runs[style_run].style.baseline_shift;
            let positioned: Vec<parley::Glyph> = glyph_run.positioned_glyphs().collect();
            let mut glyphs: Vec<Glyph> = Vec::with_capacity(positioned.len());
            let mut taken = 0;
            // The glyphs of the last cluster that had any; its ligature continuations join them.
            let mut owner = 0..0;
            while taken < positioned.len()
                || next
                    .peek()
                    .is_some_and(|c| c.glyphs == 0 && !owner.is_empty())
            {
                let Some(cluster) = next.next() else { break };
                if cluster.glyphs == 0 {
                    // A glyphless cluster that is not part of a ligature (a line break) shows nothing.
                    let joins = if cluster.continuation {
                        owner.clone()
                    } else {
                        0..0
                    };
                    for g in &mut glyphs[joins] {
                        g.cluster.end = g.cluster.end.max(cluster.source.end);
                    }
                    continue;
                }
                let own = &positioned[taken..(taken + cluster.glyphs).min(positioned.len())];
                taken += cluster.glyphs;
                if cluster.control {
                    owner = glyphs.len()..glyphs.len();
                    continue;
                }
                if !same_face || own.iter().any(|g| g.id == 0) {
                    return Err(self.missing_glyph(&cluster.source, face, style_run));
                }
                let start = glyphs.len();
                glyphs.extend(own.iter().map(|g| Glyph {
                    id: g.id,
                    x: g.x.into(),
                    y: baseline + f64::from(g.y - glyph_run.baseline()) - shift,
                    advance: g.advance.into(),
                    cluster: cluster.source.clone(),
                }));
                owner = start..glyphs.len();
            }
            runs.push(GlyphRun {
                line: line_index,
                style_run,
                face: face_index,
                font: font.clone(),
                size: f64::from(run.font_size()),
                coords: run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect(),
                glyphs,
            });
        }
        Ok(runs)
    }

    /// `range` without a closing `\n` (paragraphs are laid out without it).
    fn trim_break(&self, range: Range<usize>) -> Range<usize> {
        let end = if self.text.text[range.clone()].ends_with('\n') {
            range.end - 1
        } else {
            range.end
        };
        range.start..end
    }

    /// The error for source bytes `source` that `face` has no glyph for.
    fn missing_glyph(&self, source: &Range<usize>, face: &Face, run: usize) -> Diagnostic {
        let shown = &self.text.text[source.clone()];
        let points: Vec<String> = shown
            .chars()
            .map(|c| format!("U+{:04X}", u32::from(c)))
            .collect();
        Diagnostic::new(
            Kind::Missing,
            format!(
                "font `{}` has no glyph for `{shown}` ({})",
                face.postscript_name,
                points.join(" ")
            ),
        )
        .at(format!("runs[{run}]"))
        .expected("a font that covers every character of its run")
    }
}

#[cfg(test)]
#[path = "../tests/unit/typeset.rs"]
mod tests;
