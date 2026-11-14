//! Text set from font assets: a text layer's content handed to
//! [`motion_text`] once, at compile time, since a layout does not animate.

use std::sync::Arc;

use motion_format::Result;

use crate::animate::Parts;
use motion_format::key::{FONT, RUNS};
use motion_format::text::{Align, Caps, CharacterStyle, Leading, TextContent};
use motion_text::{
    FontQuery, FontSet, Slant, StyledRun, StyledText, TextFrame, TextLayout, Typesetter,
};

/// A text layer, set: its content and the lines and glyphs it lays out to.
#[derive(Debug, Clone, PartialEq)]
pub struct TextNode {
    /// The words and their style, as the scene wrote them.
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
/// A [`motion_format::Diagnostic`] when a run names a font the set does not
/// have or holds a character its font has no glyph for; a missing font is
/// never replaced by another.
pub(crate) fn set(content: &TextContent, typesetter: &mut Typesetter) -> Result<Arc<TextNode>> {
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
    let layout = typesetter.layout(&styled)?;
    let mut node = TextNode {
        content: content.clone(),
        layout,
        parts: Parts {
            glyphs: Vec::new(),
            counts: [0; 4],
        },
    };
    node.parts = Parts::of(&node);
    Ok(Arc::new(node))
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
        runs: content
            .runs
            .iter()
            .map(|run| StyledRun {
                range: byte_at(run.range.start)..byte_at(run.range.end),
                style: character(&run.style, fonts),
            })
            .collect(),
        paragraphs: content
            .paragraphs
            .iter()
            .map(|p| motion_text::ParagraphStyle {
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
            Some([width, height]) => TextFrame::Box {
                left: 0.0,
                top: 0.0,
                width,
                height,
            },
            None => TextFrame::Point,
        },
        small_caps_scale: motion_text::SMALL_CAPS_SCALE,
    }
}

/// A run's style as the typesetter takes it.
fn character(style: &CharacterStyle, fonts: &FontSet) -> motion_text::CharacterStyle {
    let mut out = motion_text::CharacterStyle::new(query(style, fonts), style.font.size);
    out.tracking = style.tracking;
    out.leading = match style.leading {
        Leading::Auto => motion_text::Leading::Auto,
        Leading::Px(px) => motion_text::Leading::Px(px),
    };
    out.baseline_shift = style.baseline;
    out.caps = match style.caps {
        Caps::Normal => motion_text::Caps::Normal,
        Caps::Small => motion_text::Caps::Small,
        Caps::All => motion_text::Caps::All,
        Caps::AllSmall => motion_text::Caps::AllSmall,
    };
    out.faux_bold = style.faux_bold;
    out.faux_italic = style.faux_italic;
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

fn justification(align: Align) -> motion_text::Justification {
    use motion_text::Justification as J;
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
