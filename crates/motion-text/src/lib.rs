//! Text layout for motion ([D33]): fonts from given files, shaping and line
//! breaking with Parley (HarfRust), and lines of positioned glyphs ready for
//! the renderer.
//!
//! - [`FontSet`] holds the faces text may use. They come from explicit files
//!   and folders, or the built-in Inter; never from the system or the
//!   network. A face is found by its PostScript name or by family, weight
//!   and slant.
//! - [`StyledText`] is the input: text split into paragraphs by `\n`, runs of
//!   [`CharacterStyle`], a [`ParagraphStyle`] per paragraph and a
//!   [`TextFrame`] (point or box text).
//! - [`Typesetter::layout`] turns it into a [`TextLayout`]: [`Line`]s with
//!   baselines, boxes, character ranges and cumulative character advances,
//!   and [`GlyphRun`]s whose font data is the type the renderer draws with.
//!   [`Typesetter::measure`] gives only the size.
//!
//! A font the set does not have, or a character its font has no glyph for,
//! is a [`Diagnostic`](motion_format::Diagnostic); nothing falls back silently.
//!
//! [D33]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d33-text-is-shaped-from-font-assets

mod caps;
mod font;
mod layout;
mod set;
mod style;
mod typeset;

pub use font::{Face, Slant};
pub use layout::{Glyph, GlyphRun, Line, TextLayout};
pub use set::{FontQuery, FontSet};
pub use style::{
    AUTO_LEADING, Caps, CharacterStyle, FAUX_BOLD_TRACKING, Justification, Leading, ParagraphStyle,
    SMALL_CAPS_SCALE, StyledRun, StyledText, TextFrame,
};
pub use typeset::Typesetter;

/// The font bytes in [`GlyphRun::font`]: the type the shaper and the
/// rasteriser share, from its own vocabulary crate, so the shaper stays
/// private ([D37]).
///
/// [D37]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d37-engines-stay-behind-our-own-types
pub use linebender_resource_handle::FontData;
