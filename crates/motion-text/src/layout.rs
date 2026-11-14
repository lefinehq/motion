//! What layout produces: lines and positioned glyph runs, in pixels.
//!
//! **Coordinates.** y grows downwards. For point text the origin is where the
//! first line's baseline starts, as project files store their layouts: the
//! first baseline is at y = 0, and a centred line is centred on x = 0. For
//! box text the coordinates are the box's own: its top-left corner is
//! `(left, top)` and the first baseline sits one ascent below the top.
//! Every position is a glyph's origin on its baseline, ready for a renderer.

use std::ops::Range;

use kurbo::{Rect, Size};
use parley::FontData;

/// A laid-out text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    /// The lines, top to bottom.
    pub lines: Vec<Line>,
    /// The glyph runs, line by line, in visual order within a line.
    pub runs: Vec<GlyphRun>,
}

/// One line of a [`TextLayout`].
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The paragraph it belongs to.
    pub paragraph: usize,
    /// The bytes of the source text it holds, without a closing `\n`.
    pub range: Range<usize>,
    /// How many UTF-16 code units it holds, as project files count characters.
    pub characters: usize,
    /// Where its first character starts.
    pub x: f64,
    /// Its baseline's y.
    pub baseline: f64,
    /// How far its characters advance, trailing spaces left out.
    pub width: f64,
    /// The largest ascent of its fonts, above the baseline.
    pub ascent: f64,
    /// The largest descent of its fonts, below the baseline.
    pub descent: f64,
    /// Each source character's advance, cumulative from `x`: entry `i` is
    /// where character `i + 1` starts. A ligature's advance is shared evenly
    /// by its characters.
    pub advances: Vec<f64>,
}

/// Glyphs of one font at one size on one line.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    /// The line it sits on.
    pub line: usize,
    /// The input run whose style it carries, for its paint.
    pub style_run: usize,
    /// The index of its face in the [`FontSet`](crate::FontSet).
    pub face: usize,
    /// The font bytes and face index, as the renderer takes them.
    pub font: FontData,
    /// The size, in pixels.
    pub size: f64,
    /// For a variable font, the instance's normalised axis coordinates as
    /// F2Dot14 bits; empty for a static font.
    pub coords: Vec<i16>,
    /// The glyphs, in visual order.
    pub glyphs: Vec<Glyph>,
}

/// One positioned glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    /// Its id in the font.
    pub id: u32,
    /// Its origin's x.
    pub x: f64,
    /// Its origin's y: the line's baseline, less any baseline shift.
    pub y: f64,
    /// How far it advances, tracking included.
    pub advance: f64,
    /// The bytes of the source text it shows.
    pub cluster: Range<usize>,
}

impl TextLayout {
    /// The box the lines cover: each from its start to its width, from its
    /// ascent to its descent; an empty box at the origin when there are none.
    pub fn bounds(&self) -> Rect {
        self.lines
            .iter()
            .map(|l| {
                Rect::new(
                    l.x,
                    l.baseline - l.ascent,
                    l.x + l.width,
                    l.baseline + l.descent,
                )
            })
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO)
    }

    /// The size of [`TextLayout::bounds`].
    pub fn size(&self) -> Size {
        self.bounds().size()
    }
}

#[cfg(test)]
#[path = "../tests/unit/layout.rs"]
mod tests;
