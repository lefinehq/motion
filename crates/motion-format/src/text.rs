//! Text style: the character and paragraph keys a text layer's `style`, its
//! `runs` and its `paragraphs` set, as one table ([`STYLE_KEYS`]) of names,
//! levels, accepted values and typed setters.

use std::ops::Range;

use serde_json::Value as Json;

use crate::doc::{Number, Shorthand, ValueDoc};
use crate::item::Item;
use crate::shorthand::Font;
use crate::table::{self, Named};
use crate::value::{Value, ValueType};
use crate::{Diagnostic, Kind, Result, fmt_num};

/// A text layer's content: the words and how they are set.
#[derive(Debug, Clone, PartialEq)]
pub struct TextContent {
    /// The words; `\n` ends a paragraph.
    pub text: String,
    /// The style of the whole text, before any run.
    pub base: CharacterStyle,
    /// The style of every character, as runs that cover the text in order.
    pub runs: Vec<TextRun>,
    /// One style per paragraph, in order.
    pub paragraphs: Vec<ParagraphStyle>,
    /// Box text's box in the layer's own coordinates, or `None` for point
    /// text.
    pub frame: Option<kurbo::Rect>,
    /// Its animators, applied in order.
    pub animators: Vec<Item>,
}

impl TextContent {
    /// The style of the first character, or the base style of empty text.
    pub fn first_style(&self) -> &CharacterStyle {
        &self.runs[0].style
    }
}

/// Characters that share one style; the range counts Unicode scalar values.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// The characters, as indices of `char`s.
    pub range: Range<usize>,
    /// Their style.
    pub style: CharacterStyle,
    /// The index in the layer's `runs` of the last run that styled them, or
    /// `None` for the base style.
    pub source: Option<usize>,
}

/// The style of a run of characters.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterStyle {
    /// Family or PostScript name, size and weight.
    pub font: Font,
    /// The italic face of the family, rather than its upright one.
    pub italic: bool,
    /// The colour inside the glyphs, straight RGBA from 0 to 1.
    pub fill: [f64; 4],
    /// The colour along the glyphs' outlines; clear draws no stroke.
    pub stroke: [f64; 4],
    /// The stroke's width in px, centred on the outline.
    pub stroke_width: f64,
    /// Which of fill and stroke is drawn on top.
    pub paint: PaintOrder,
    /// Extra space after each character, in thousandths of an em.
    pub tracking: f64,
    /// The distance from the previous baseline.
    pub leading: Leading,
    /// How far the baseline is raised, in px.
    pub baseline: f64,
    /// Capitals.
    pub caps: Caps,
    /// A bold drawn by thickening the glyphs, for a face without a bold.
    pub faux_bold: bool,
    /// An italic drawn by slanting the glyphs, for a face without an italic.
    pub faux_italic: bool,
}

impl CharacterStyle {
    /// `font` filled with `fill`, with every other key at its default.
    pub fn new(font: Font, fill: [f64; 4]) -> CharacterStyle {
        CharacterStyle {
            font,
            italic: false,
            fill,
            stroke: [0.0; 4],
            stroke_width: 1.0,
            paint: PaintOrder::FillOver,
            tracking: 0.0,
            leading: Leading::Auto,
            baseline: 0.0,
            caps: Caps::Normal,
            faux_bold: false,
            faux_italic: false,
        }
    }
}

/// The style of a paragraph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParagraphStyle {
    /// How its lines align.
    pub align: Align,
    /// The first line's extra indent, in px.
    pub indent_first: f64,
    /// The indent at the start of every line, in px.
    pub indent_start: f64,
    /// The indent at the end of every line of box text, in px.
    pub indent_end: f64,
    /// Space above the paragraph, after the first, in px.
    pub space_before: f64,
    /// Space below the paragraph, before the next, in px.
    pub space_after: f64,
    /// Automatic leading, as a multiple of the size.
    pub auto_leading: f64,
}

impl Default for ParagraphStyle {
    fn default() -> ParagraphStyle {
        ParagraphStyle {
            align: Align::Left,
            indent_first: 0.0,
            indent_start: 0.0,
            indent_end: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            auto_leading: 1.2,
        }
    }
}

/// The distance from one baseline to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leading {
    /// The paragraph's automatic leading times the size.
    Auto,
    /// A fixed distance, in px.
    Px(f64),
}

/// Which paint is on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintOrder {
    /// The fill over the stroke, so the stroke shows only outside the glyph.
    FillOver,
    /// The stroke over the fill.
    StrokeOver,
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

/// How a paragraph's lines align.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Flush left.
    Left,
    /// Flush right.
    Right,
    /// Centred.
    Center,
    /// Justified, the last line flush left.
    JustifyLeft,
    /// Justified, the last line flush right.
    JustifyRight,
    /// Justified, the last line centred.
    JustifyCenter,
    /// Justified, the last line too.
    JustifyAll,
}

/// The names of [`PaintOrder`].
pub const PAINT_ORDERS: [(&str, PaintOrder); 2] = [
    ("fill-over", PaintOrder::FillOver),
    ("stroke-over", PaintOrder::StrokeOver),
];

/// The names of [`Caps`].
pub const CAPS: [(&str, Caps); 4] = [
    ("normal", Caps::Normal),
    ("small", Caps::Small),
    ("all", Caps::All),
    ("all-small", Caps::AllSmall),
];

/// The names of [`Align`].
pub const ALIGNS: [(&str, Align); 7] = [
    ("left", Align::Left),
    ("right", Align::Right),
    ("center", Align::Center),
    ("justify-left", Align::JustifyLeft),
    ("justify-right", Align::JustifyRight),
    ("justify-center", Align::JustifyCenter),
    ("justify-all", Align::JustifyAll),
];

/// Whether a key styles characters or paragraphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// It may be set on the whole text and on runs.
    Character,
    /// It may be set on the whole text and on paragraphs.
    Paragraph,
}

/// What a style key sets.
pub enum Setter {
    /// A key of a character style.
    Character(fn(&mut CharacterStyle, &Json) -> Result<()>),
    /// A key of a paragraph style.
    Paragraph(fn(&mut ParagraphStyle, &Json) -> Result<()>),
}

/// One style key.
pub struct StyleKey {
    /// The key scene files write.
    pub name: &'static str,
    /// The values it accepts, with an example.
    pub form: &'static str,
    /// What it means, for people and agents.
    pub doc: &'static str,
    /// What it sets.
    pub set: Setter,
}

impl StyleKey {
    /// Whether it styles characters or paragraphs.
    pub fn level(&self) -> Level {
        match self.set {
            Setter::Character(_) => Level::Character,
            Setter::Paragraph(_) => Level::Paragraph,
        }
    }
}

impl std::fmt::Debug for StyleKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StyleKey")
            .field("name", &self.name)
            .field("level", &self.level())
            .finish_non_exhaustive()
    }
}

impl Named for StyleKey {
    fn name(&self) -> &str {
        self.name
    }
}

/// Reads `written` as a finite number of at least `lo`.
fn at_least(written: &Json, lo: f64) -> Result<f64> {
    let Number(n) = crate::de::from_value(written)?;
    if n.is_finite() && n >= lo {
        return Ok(n);
    }
    Err(Diagnostic::new(
        Kind::OutOfRange,
        format!("{} is below {}", fmt_num(n), fmt_num(lo)),
    )
    .expected(format!("a number of at least {}", fmt_num(lo))))
}

/// Reads `written` as a finite number.
fn finite(written: &Json) -> Result<f64> {
    at_least(written, f64::MIN)
}

/// Reads `written` as a colour, in any form a colour value takes.
pub(crate) fn color(written: &Json) -> Result<[f64; 4]> {
    let doc: ValueDoc = crate::de::from_value(written)?;
    let value = match &doc {
        ValueDoc::Numbers(numbers) => Value::from_numbers(numbers, ValueType::Color, false)?,
        ValueDoc::Text(text) => Value::parse(text, ValueType::Color, false)?,
    };
    let c = value.components();
    if c.iter().all(|v| (0.0..=1.0).contains(v)) {
        return Ok([c[0], c[1], c[2], c[3]]);
    }
    Err(
        Diagnostic::new(Kind::OutOfRange, "a colour channel is outside 0 to 1")
            .expected(ValueType::Color.form()),
    )
}

/// Reads `written` as one of `names`.
fn choice<T: Copy>(written: &Json, what: &str, names: &'static [(&'static str, T)]) -> Result<T> {
    let name: String = crate::de::from_value(written)?;
    table::lookup(names, &name)
        .map(|(_, value)| *value)
        .ok_or_else(|| Diagnostic::unknown(what, &name, table::names(names)))
}

/// Reads `written` as `true` or `false`.
fn flag(written: &Json) -> Result<bool> {
    crate::de::from_value(written)
}

/// Every style key, characters first.
pub static STYLE_KEYS: [StyleKey; 19] = [
    StyleKey {
        name: "font",
        form: "`<family or PostScript name> <size> [<weight>]`, e.g. `Lato-Black 55`",
        doc: "the font, by family and weight or by PostScript name, and its size in px",
        set: Setter::Character(|s, j| {
            s.font = crate::de::from_value::<Shorthand<Font>>(j)?.parse()?;
            Ok(())
        }),
    },
    StyleKey {
        name: "italic",
        form: "`true` or `false`",
        doc: "the family's italic face rather than its upright one",
        set: Setter::Character(|s, j| {
            s.italic = flag(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "fill",
        form: "a colour, e.g. `#FFFFFF` or `[1, 1, 1, 1]`",
        doc: "the colour inside the glyphs",
        set: Setter::Character(|s, j| {
            s.fill = color(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "stroke",
        form: "a colour, e.g. `#8F8F8F`",
        doc: "the colour along the glyphs' outlines; none unless set",
        set: Setter::Character(|s, j| {
            s.stroke = color(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "stroke-width",
        form: "a number of px, at least 0",
        doc: "the stroke's width, centred on the outline; 1 by default",
        set: Setter::Character(|s, j| {
            s.stroke_width = at_least(j, 0.0)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "paint",
        form: "`fill-over` or `stroke-over`",
        doc: "which of fill and stroke is drawn on top; the fill by default",
        set: Setter::Character(|s, j| {
            s.paint = choice(j, "paint order", &PAINT_ORDERS)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "tracking",
        form: "a number, in thousandths of an em",
        doc: "extra space after each character",
        set: Setter::Character(|s, j| {
            s.tracking = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "leading",
        form: "`auto` or a number of px",
        doc: "the distance from the previous baseline; the paragraph's automatic leading by default",
        set: Setter::Character(|s, j| {
            s.leading = match j.as_str() {
                Some("auto") => Leading::Auto,
                _ => Leading::Px(
                    at_least(j, 0.0).map_err(|e| e.expected("`auto` or a number of px"))?,
                ),
            };
            Ok(())
        }),
    },
    StyleKey {
        name: "baseline",
        form: "a number of px",
        doc: "how far the baseline is raised",
        set: Setter::Character(|s, j| {
            s.baseline = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "caps",
        form: "`normal`, `small`, `all` or `all-small`",
        doc: "capitals: small capitals for lower case, every letter a capital, or every letter a small capital",
        set: Setter::Character(|s, j| {
            s.caps = choice(j, "caps", &CAPS)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "faux-bold",
        form: "`true` or `false`",
        doc: "thickens the glyphs, for a face without a bold",
        set: Setter::Character(|s, j| {
            s.faux_bold = flag(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "faux-italic",
        form: "`true` or `false`",
        doc: "slants the glyphs, for a face without an italic",
        set: Setter::Character(|s, j| {
            s.faux_italic = flag(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "align",
        form: "`left`, `right`, `center`, `justify-left`, `justify-right`, `justify-center` or `justify-all`",
        doc: "how the lines align; point text aligns about its origin",
        set: Setter::Paragraph(|s, j| {
            s.align = choice(j, "alignment", &ALIGNS)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "indent-first",
        form: "a number of px",
        doc: "the first line's extra indent",
        set: Setter::Paragraph(|s, j| {
            s.indent_first = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "indent-start",
        form: "a number of px",
        doc: "the indent at the start of every line",
        set: Setter::Paragraph(|s, j| {
            s.indent_start = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "indent-end",
        form: "a number of px",
        doc: "the indent at the end of every line of box text",
        set: Setter::Paragraph(|s, j| {
            s.indent_end = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "space-before",
        form: "a number of px",
        doc: "space above the paragraph, after the first",
        set: Setter::Paragraph(|s, j| {
            s.space_before = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "space-after",
        form: "a number of px",
        doc: "space below the paragraph, before the next",
        set: Setter::Paragraph(|s, j| {
            s.space_after = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "auto-leading",
        form: "a number, at least 0",
        doc: "automatic leading as a multiple of the size; 1.2 by default",
        set: Setter::Paragraph(|s, j| {
            s.auto_leading = at_least(j, 0.0)?;
            Ok(())
        }),
    },
];

/// The style key called `name`, if there is one.
pub fn style_key(name: &str) -> Option<&'static StyleKey> {
    table::lookup(&STYLE_KEYS, name)
}

#[cfg(test)]
#[path = "../tests/unit/text.rs"]
mod tests;
