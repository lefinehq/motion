//! Text set from font assets: a text layer's content handed to
//! [`nakkash_text`] once, at compile time, since a layout does not animate.

use std::collections::HashMap;
use std::sync::Arc;

use nakkash_format::Result;

use crate::animate::Parts;
use nakkash_format::key::{ANIM, FONT, RUNS, TEXT};
use nakkash_format::text::{Align, Caps, CharacterStyle, Kerning, Leading, TextContent, TextFit};
use nakkash_text::{
    Fit, FontQuery, FontSet, Slant, StyledRun, StyledText, TextFrame, TextLayout, Typesetter,
};

/// A text layer, set: its content and the lines and glyphs it lays out to.
#[derive(Debug, Clone, PartialEq)]
pub struct TextNode {
    /// The words and their style, as the scene wrote them, with every
    /// length that goes with the size scaled as the layout's fit scaled it
    /// ([`TextLayout::scale`]).
    pub content: TextContent,
    /// The lines and glyph runs, in the layer's own coordinates: point
    /// text's first baseline starts at the origin, box text's box does.
    pub layout: TextLayout,
    /// Which character, word and line each glyph belongs to, for animators.
    pub parts: Parts,
}

impl TextNode {
    /// The style of glyph run `run` of the layout.
    pub fn style_of(&self, run: usize) -> &CharacterStyle {
        &self.content.runs[self.layout.runs[run].style_run].style
    }
}

/// Lays out `content` with `typesetter`.
///
/// # Errors
/// A [`nakkash_format::Diagnostic`] when a run names a font the set does not
/// have or holds a character its font has no glyph for; a missing font is
/// never replaced by another.
pub fn set(content: &TextContent, typesetter: &mut Typesetter) -> Result<Arc<TextNode>> {
    set_with(content, typesetter, None)
}

/// A text layer's own words and the words of each of its keys ([D92]), set:
/// its own as [`set`] sets them, and each key's in its own layout, in the
/// layer's style ([`TextContent::with_words`]), at the frame it starts to
/// hold in the layer's own time at `fps`. A fit sets them all at one
/// scale, the smallest any of them takes on its own, so the size does not
/// change from key to key. Keys of the same words share one layout.
///
/// # Errors
/// As [`set`], at the key whose words break a rule.
///
/// [D92]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d92-a-text-layers-words-may-be-keyed
pub fn set_keyed(
    content: &TextContent,
    typesetter: &mut Typesetter,
    fps: f64,
) -> Result<(Arc<TextNode>, Vec<TextKeyNode>)> {
    let own = set(content, typesetter)?;
    if content.keys.is_empty() {
        return Ok((own, Vec::new()));
    }
    let mut by_words: HashMap<&str, Arc<TextNode>> = HashMap::new();
    for (i, key) in content.keys.iter().enumerate() {
        if !by_words.contains_key(key.text.as_str()) {
            let node = set(&content.with_words(&key.text), typesetter)
                .map_err(|e| e.under(format!("[{i}]")).under(TEXT).under(ANIM))?;
            by_words.insert(&key.text, node);
        }
    }
    // One scale for all, the smallest any takes; without a fit every
    // scale is 1.
    let scale = by_words
        .values()
        .chain(std::iter::once(&own))
        .map(|node| node.layout.scale)
        .fold(1.0_f64, f64::min);
    let mut at_scale = |node: Arc<TextNode>, content: &TextContent| match node.layout.scale > scale
    {
        true => set_with(content, typesetter, Some(scale)),
        false => Ok(node),
    };
    let own = at_scale(own, content)?;
    let mut set_words = HashMap::with_capacity(by_words.len());
    for (words, node) in by_words {
        set_words.insert(words, at_scale(node, &content.with_words(words))?);
    }
    let keys = content
        .keys
        .iter()
        .map(|key| TextKeyNode {
            frame: key.time.to_frames(fps),
            text: Arc::clone(&set_words[key.text.as_str()]),
        })
        .collect();
    Ok((own, keys))
}

/// A key of a text node's words, set: from `frame` of the layer's own
/// time on, until the next key, it shows `text`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextKeyNode {
    /// When it starts to hold, in frames of the layer's own time.
    pub frame: f64,
    /// Its words, their style and their layout.
    pub text: Arc<TextNode>,
}

/// How close to a key's frame a frame drawn counts as reached: imported key
/// times are written to the microsecond, so a frame the original changed
/// the words at may come out a hair before its key.
const REACHED: f64 = 1e-3;

/// The text a node with its own words `own` and keys `keys` shows at
/// `local`, a frame of its own time: the last key reached, the first
/// before any, and its own without keys.
pub fn shown<'t>(own: &'t Arc<TextNode>, keys: &'t [TextKeyNode], local: f64) -> &'t Arc<TextNode> {
    match keys.partition_point(|key| key.frame <= local + REACHED) {
        _ if keys.is_empty() => own,
        0 => &keys[0].text,
        after => &keys[after - 1].text,
    }
}

/// [`set`], at `scale` in place of the scale its fit finds when one is given.
fn set_with(
    content: &TextContent,
    typesetter: &mut Typesetter,
    scale: Option<f64>,
) -> Result<Arc<TextNode>> {
    let styled = styled(content, typesetter.fonts());
    // Find every face first, so a missing one is reported at the scene key that names it.
    for (run, asked) in content.runs.iter().zip(&styled.runs) {
        typesetter
            .fonts()
            .position(&asked.style.font)
            .map_err(|e| {
                let at = match run.source {
                    Some(i)
                        if run.style.font != content.base.font
                            || run.style.italic != content.base.italic =>
                    {
                        format!("{RUNS}[{i}]")
                    }
                    _ => String::new(),
                };
                e.at(FONT).under(at)
            })?;
    }
    let layout = match scale {
        Some(scale) => typesetter.layout_at(&styled, scale)?,
        None => typesetter.layout(&styled)?,
    };
    let mut node = TextNode {
        // A fit that scaled the text scales its stroke and the sizes animators read too.
        content: if layout.scale == 1.0 {
            content.clone()
        } else {
            content.scaled(layout.scale)
        },
        layout,
        parts: Parts {
            glyphs: Vec::new(),
            counts: [0; 4],
        },
    };
    node.parts = Parts::of(&node);
    Ok(Arc::new(node))
}

/// Lays out `content` as written, leaving its `fit` out: the lines whose
/// size an import writes as the fit's size, so the original text fits at
/// scale 1.
///
/// # Errors
/// As [`set`].
pub fn set_as_written(content: &TextContent, typesetter: &mut Typesetter) -> Result<Arc<TextNode>> {
    let written = TextContent {
        fit: None,
        ..content.clone()
    };
    set(&written, typesetter)
}

/// `content` as the typesetter takes it: byte ranges, font queries and its enums.
fn styled(content: &TextContent, fonts: &FontSet) -> StyledText {
    // Runs count characters; the typesetter counts bytes.
    let byte_at = |chars: usize| {
        content
            .text
            .char_indices()
            .nth(chars)
            .map_or(content.text.len(), |(at, _)| at)
    };
    StyledText {
        text: content.text.clone(),
        // The format keeps one base run of no character for an empty text,
        // to carry its style; the typesetter takes no run of no character.
        runs: content
            .runs
            .iter()
            .filter(|run| !run.range.is_empty())
            .map(|run| StyledRun {
                range: byte_at(run.range.start)..byte_at(run.range.end),
                style: character(&run.style, fonts),
            })
            .collect(),
        paragraphs: content
            .paragraphs
            .iter()
            .map(|p| nakkash_text::ParagraphStyle {
                justification: justification(p.align),
                first_line_indent: p.indent_first,
                start_indent: p.indent_start,
                end_indent: p.indent_end,
                space_before: p.space_before,
                space_after: p.space_after,
                auto_leading: p.auto_leading,
            })
            .collect(),
        frame: match content.frame {
            Some(frame) => TextFrame::Box {
                left: frame.x0,
                top: frame.y0,
                width: frame.width(),
                height: frame.height(),
            },
            None => TextFrame::Point,
        },
        fit: content.fit.map(|fit| match fit {
            TextFit::Shrink { width, height } => Fit::Shrink { width, height },
            TextFit::Wrap { width, height } => Fit::Wrap { width, height },
            TextFit::ShrinkToBox => Fit::Box,
        }),
        small_caps_scale: nakkash_text::SMALL_CAPS_SCALE,
    }
}

/// A run's style as the typesetter takes it.
fn character(style: &CharacterStyle, fonts: &FontSet) -> nakkash_text::CharacterStyle {
    let mut out = nakkash_text::CharacterStyle::new(query(style, fonts), style.font.size);
    out.tracking = style.tracking;
    out.horizontal_scale = style.scale_x;
    out.vertical_scale = style.scale_y;
    out.kerning = style.kerning == Kerning::Font;
    out.leading = match style.leading {
        Leading::Auto => nakkash_text::Leading::Auto,
        Leading::Px(px) => nakkash_text::Leading::Px(px),
    };
    out.baseline_shift = style.baseline;
    out.caps = match style.caps {
        Caps::Normal => nakkash_text::Caps::Normal,
        Caps::Small => nakkash_text::Caps::Small,
        Caps::All => nakkash_text::Caps::All,
        Caps::AllSmall => nakkash_text::Caps::AllSmall,
    };
    out.faux_bold = style.faux_bold;
    out.faux_italic = style.faux_italic;
    out.lang.clone_from(&style.lang);
    out
}

/// How a style asks for its face: by PostScript name when the font's name
/// is one the set has (`Lato-Black`), else by family, weight and slant.
fn query(style: &CharacterStyle, fonts: &FontSet) -> FontQuery {
    let name = &style.font.family;
    if fonts.faces().iter().any(|f| &f.postscript_name == name) {
        return FontQuery::PostScript(name.clone());
    }
    FontQuery::Family {
        family: name.clone(),
        weight: f32::from(style.font.weight),
        slant: match style.italic {
            true => Slant::Italic,
            false => Slant::Normal,
        },
    }
}

fn justification(align: Align) -> nakkash_text::Justification {
    use nakkash_text::Justification as J;
    match align {
        Align::Left => J::Left,
        Align::Right => J::Right,
        Align::Center => J::Center,
        Align::JustifyLeft => J::JustifyLastLeft,
        Align::JustifyRight => J::JustifyLastRight,
        Align::JustifyCenter => J::JustifyLastCenter,
        Align::JustifyAll => J::JustifyAll,
    }
}

#[cfg(test)]
#[path = "../tests/unit/text.rs"]
mod tests;
