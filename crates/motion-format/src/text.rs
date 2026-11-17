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
    /// What the whole text sets once: the anchors its parts take and how
    /// its paints stack.
    pub whole: WholeStyle,
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
    /// How much wider than the font draws them the characters are set:
    /// their advances, tracking and glyphs scale across by it.
    pub scale_x: f64,
    /// How much taller than the font draws them the glyphs are drawn, up from
    /// their baseline; the lines' spacing does not change.
    pub scale_y: f64,
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
    /// The language of the words, as a tag such as `tr` or `pt-BR`: how
    /// capitals are made and which of the font's local forms are used.
    pub lang: Option<String>,
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
            scale_x: 1.0,
            scale_y: 1.0,
            leading: Leading::Auto,
            baseline: 0.0,
            caps: Caps::Normal,
            faux_bold: false,
            faux_italic: false,
            lang: None,
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

/// What a text sets once for all its characters: the anchor each part
/// scales and turns about, and how fills and strokes stack across
/// characters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WholeStyle {
    /// Whose anchor each character takes: its own, its word's, its line's
    /// or the whole text's.
    pub anchor_by: AnchorBy,
    /// How far every anchor moves from the middle of its baseline, as
    /// fractions of its part's width and of its font's size: `[-0.5, 0]`
    /// is the start of the part's baseline, `[0, -0.5]` half a size above
    /// it.
    pub anchor_align: [f64; 2],
    /// How fills and strokes stack across characters.
    pub paint_across: PaintAcross,
}

impl Default for WholeStyle {
    fn default() -> WholeStyle {
        WholeStyle {
            anchor_by: AnchorBy::Char,
            anchor_align: [0.0, 0.0],
            paint_across: PaintAcross::Each,
        }
    }
}

/// Whose anchor each character scales and turns about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorBy {
    /// Its own: the middle of its own baseline.
    Char,
    /// Its word's: the middle of the word's baseline.
    Word,
    /// Its line's: the middle of the line's baseline.
    Line,
    /// The whole text's: the middle of its first baseline across all its lines.
    All,
}

/// How fills and strokes stack across characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintAcross {
    /// Character by character, each in its own `paint` order, so a later
    /// character covers an earlier one.
    Each,
    /// Every stroke first, then every fill, so no stroke covers a fill.
    FillsOver,
    /// Every fill first, then every stroke, so no fill covers a stroke.
    StrokesOver,
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

/// The names of [`AnchorBy`].
pub const ANCHORS_BY: [(&str, AnchorBy); 4] = [
    ("char", AnchorBy::Char),
    ("word", AnchorBy::Word),
    ("line", AnchorBy::Line),
    ("all", AnchorBy::All),
];

/// The names of [`PaintAcross`].
pub const PAINTS_ACROSS: [(&str, PaintAcross); 3] = [
    ("each", PaintAcross::Each),
    ("fills-over", PaintAcross::FillsOver),
    ("strokes-over", PaintAcross::StrokesOver),
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

/// Whether a key styles characters, paragraphs or the whole text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// It may be set on the whole text and on runs.
    Character,
    /// It may be set on the whole text and on paragraphs.
    Paragraph,
    /// It may be set on the whole text only.
    Text,
}

/// What a style key sets.
pub enum Setter {
    /// A key of a character style.
    Character(fn(&mut CharacterStyle, &Json) -> Result<()>),
    /// A key of a paragraph style.
    Paragraph(fn(&mut ParagraphStyle, &Json) -> Result<()>),
    /// A key of the whole text.
    Text(fn(&mut WholeStyle, &Json) -> Result<()>),
}

/// One style key.
pub struct StyleKey {
    /// The key scene files write.
    pub name: &'static str,
    /// The values it accepts, with an example.
    pub form: &'static str,
    /// What it means, for people and agents.
    pub doc: &'static str,
    /// Whether the layer itself sets it, never a style: the base font and
    /// fill every text writes. Runs may still change it.
    pub on_layer: bool,
    /// What it sets.
    pub set: Setter,
}

impl StyleKey {
    /// Whether `value` changes nothing on a style that leaves this key out,
    /// so a writer may leave it out too. A key the layer sets has no
    /// default; a value the key rejects is not one.
    pub fn is_default(&self, value: &Json) -> bool {
        if self.on_layer {
            return false;
        }
        match self.set {
            Setter::Character(set) => {
                // The font and fill are the layer's, never compared here.
                let font = Font {
                    family: String::new(),
                    size: 1.0,
                    weight: 400,
                };
                let unset = CharacterStyle::new(font, [0.0; 4]);
                let mut style = unset.clone();
                set(&mut style, value).is_ok() && style == unset
            }
            Setter::Paragraph(set) => {
                let mut style = ParagraphStyle::default();
                set(&mut style, value).is_ok() && style == ParagraphStyle::default()
            }
            Setter::Text(set) => {
                let mut style = WholeStyle::default();
                set(&mut style, value).is_ok() && style == WholeStyle::default()
            }
        }
    }

    /// Whether it styles characters, paragraphs or the whole text.
    pub fn level(&self) -> Level {
        match self.set {
            Setter::Character(_) => Level::Character,
            Setter::Paragraph(_) => Level::Paragraph,
            Setter::Text(_) => Level::Text,
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

/// Reads `written` as a finite number above zero.
fn above_zero(written: &Json) -> Result<f64> {
    let n = at_least(written, 0.0)?;
    if n > 0.0 {
        return Ok(n);
    }
    Err(Diagnostic::new(Kind::OutOfRange, "0 is not above 0").expected("a number above 0"))
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

/// Every style key: characters, then paragraphs, then the whole text.
pub static STYLE_KEYS: [StyleKey; 25] = [
    StyleKey {
        name: "font",
        form: "`<family or PostScript name> <size> [<weight>]`, e.g. `Lato-Black 55`",
        doc: "the font, by family and weight or by PostScript name, and its size in px",
        on_layer: true,
        set: Setter::Character(|s, j| {
            s.font = crate::de::from_value::<Shorthand<Font>>(j)?.parse()?;
            Ok(())
        }),
    },
    StyleKey {
        name: "italic",
        form: "`true` or `false`",
        doc: "the family's italic face rather than its upright one",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.italic = flag(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "fill",
        form: "a colour, e.g. `#FFFFFF` or `[1, 1, 1, 1]`",
        doc: "the colour inside the glyphs",
        on_layer: true,
        set: Setter::Character(|s, j| {
            s.fill = color(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "stroke",
        form: "a colour, e.g. `#8F8F8F`",
        doc: "the colour along the glyphs' outlines; none unless set",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.stroke = color(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "stroke-width",
        form: "a number of px, at least 0",
        doc: "the stroke's width, centred on the outline; 1 by default",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.stroke_width = at_least(j, 0.0)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "paint",
        form: "`fill-over` or `stroke-over`",
        doc: "which of fill and stroke is drawn on top; the fill by default",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.paint = choice(j, "paint order", &PAINT_ORDERS)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "tracking",
        form: "a number, in thousandths of an em",
        doc: "extra space after each character",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.tracking = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "horizontal-scale",
        form: "a number above 0, e.g. `0.93`",
        doc: "how much wider than the font draws them the characters are set: advances, tracking and glyphs; 1 by default",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.scale_x = above_zero(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "vertical-scale",
        form: "a number above 0, e.g. `1.04`",
        doc: "how much taller than the font draws them the glyphs are drawn, up from the baseline; the lines' spacing stays; 1 by default",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.scale_y = above_zero(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "leading",
        form: "`auto` or a number of px",
        doc: "the distance from the previous baseline; the paragraph's automatic leading by default",
        on_layer: false,
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
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.baseline = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "caps",
        form: "`normal`, `small`, `all` or `all-small`",
        doc: "capitals: small capitals for lower case, every letter a capital, or every letter a small capital",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.caps = choice(j, "caps", &CAPS)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "faux-bold",
        form: "`true` or `false`",
        doc: "thickens the glyphs, for a face without a bold",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.faux_bold = flag(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "faux-italic",
        form: "`true` or `false`",
        doc: "slants the glyphs, for a face without an italic",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.faux_italic = flag(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "lang",
        form: "a language tag such as `tr` or `pt-BR`",
        doc: "the language of the words: capitals in Turkish and Azerbaijani keep the dot on i (İ), and the font's local forms for the language are used",
        on_layer: false,
        set: Setter::Character(|s, j| {
            s.lang = Some(language(j)?);
            Ok(())
        }),
    },
    StyleKey {
        name: "align",
        form: "`left`, `right`, `center`, `justify-left`, `justify-right`, `justify-center` or `justify-all`",
        doc: "how the lines align; point text aligns about its origin",
        on_layer: false,
        set: Setter::Paragraph(|s, j| {
            s.align = choice(j, "alignment", &ALIGNS)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "indent-first",
        form: "a number of px",
        doc: "the first line's extra indent",
        on_layer: false,
        set: Setter::Paragraph(|s, j| {
            s.indent_first = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "indent-start",
        form: "a number of px",
        doc: "the indent at the start of every line",
        on_layer: false,
        set: Setter::Paragraph(|s, j| {
            s.indent_start = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "indent-end",
        form: "a number of px",
        doc: "the indent at the end of every line of box text",
        on_layer: false,
        set: Setter::Paragraph(|s, j| {
            s.indent_end = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "space-before",
        form: "a number of px",
        doc: "space above the paragraph, after the first",
        on_layer: false,
        set: Setter::Paragraph(|s, j| {
            s.space_before = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "space-after",
        form: "a number of px",
        doc: "space below the paragraph, before the next",
        on_layer: false,
        set: Setter::Paragraph(|s, j| {
            s.space_after = finite(j)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "auto-leading",
        form: "a number, at least 0",
        doc: "automatic leading as a multiple of the size; 1.2 by default",
        on_layer: false,
        set: Setter::Paragraph(|s, j| {
            s.auto_leading = at_least(j, 0.0)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "anchor-by",
        form: "`char`, `word`, `line` or `all`",
        doc: "whose anchor each character scales and turns about under animators: its own, its word's, its line's or the whole text's; its own by default",
        on_layer: false,
        set: Setter::Text(|s, j| {
            s.anchor_by = choice(j, "anchor grouping", &ANCHORS_BY)?;
            Ok(())
        }),
    },
    StyleKey {
        name: "anchor-align",
        form: "`[x, y]`, fractions, e.g. `[0, -0.3]`",
        doc: "how far every part's anchor moves from the middle of its baseline: across by fractions of the part's width, down by fractions of its font's size",
        on_layer: false,
        set: Setter::Text(|s, j| {
            let [Number(x), Number(y)]: [Number; 2] = crate::de::from_value(j)?;
            if !(x.is_finite() && y.is_finite()) {
                return Err(Diagnostic::new(Kind::Invalid, "a fraction is not finite")
                    .expected("two finite numbers"));
            }
            s.anchor_align = [x, y];
            Ok(())
        }),
    },
    StyleKey {
        name: "paint-across",
        form: "`each`, `fills-over` or `strokes-over`",
        doc: "how fills and strokes stack across characters: each character in its own `paint` order, every fill over every stroke, or every stroke over every fill; each by default",
        on_layer: false,
        set: Setter::Text(|s, j| {
            s.paint_across = choice(j, "paint across", &PAINTS_ACROSS)?;
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

/// A language tag: a language of two or three letters, then subtags of
/// one to eight letters or digits, each after a `-`.
fn language(j: &Json) -> Result<String> {
    let tag = j.as_str().unwrap_or_default();
    let mut parts = tag.split('-');
    let first = parts.next().unwrap_or_default();
    let fits = (2..=3).contains(&first.len())
        && first.chars().all(|c| c.is_ascii_alphabetic())
        && parts
            .all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()));
    match fits {
        true => Ok(tag.to_owned()),
        false => Err(
            Diagnostic::new(Kind::WrongType, format!("`{j}` is not a language tag"))
                .expected("a language tag such as `tr` or `pt-BR`"),
        ),
    }
}
