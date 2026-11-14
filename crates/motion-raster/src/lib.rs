//! Step 4 (Draw): turns a frame's display list into pixels with `vello_cpu`
//! ([D2], [D34]).
//!
//! A [`Frame`] is a tree of [`Layer`]s with paths and glyph runs as leaves.
//! [`render`] draws it into an [`Image`], which [`Image::to_png`] encodes.
//! Effects are rows of [`EFFECTS`]; blend modes, caps, joins and fill rules
//! are tables too, so callers map names to them without branching.
//!
//! The crate knows nothing about scenes or time: evaluating a scene at a
//! frame and writing its display list happens before it.
//!
//! [D2]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d2-vello-as-the-rasteriser
//! [D34]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

mod blend;
mod effect;
mod image;
mod list;
mod render;
mod style;

pub use blend::{BLEND_MODES, BlendMode, BlendSpec};
pub use effect::{EFFECTS, Effect, EffectContext, EffectSpec, ParamSpec, ParamType, ParamValue};
pub use image::Image;
pub use list::{Blending, Clip, Frame, Glyph, GlyphDraw, Layer, Matte, MatteMode, Node, PathDraw};
pub use render::{Quality, RenderOptions, Simd, render, render_with};
pub use style::{
    CAPS, Cap, Color, Dashes, DrawStyle, FILL_RULES, FillRule, JOINS, Join, Paint, StrokeStyle,
};
/// The geometry and font types display lists are made of: vocabulary crates
/// the rasteriser shares, named from themselves so it stays private ([D37]).
///
/// [D37]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d37-engines-stay-behind-our-own-types
pub use {kurbo, peniko};
