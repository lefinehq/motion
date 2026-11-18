//! Step 4 (Draw): turns a frame's display list into pixels with `vello_cpu`
//! ([D2], [D34]).
//!
//! A [`Frame`] is a tree of [`Layer`]s with paths, glyph runs and pictures
//! as leaves; a [`Picture`] is an image file decoded once, for every frame
//! that draws it.
//! [`render`] draws it into an [`Image`], which [`Image::to_png`] encodes.
//! Effects are rows of [`EFFECTS`]; blend modes, caps, joins and fill rules
//! are tables too, so callers map names to them without branching.
//!
//! The crate knows nothing about scenes or time: evaluating a scene at a
//! frame and writing its display list happens before it.
//!
//! [D2]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d2-vello-as-the-rasteriser
//! [D34]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

mod accel;
mod blend;
mod blur;
mod channel;
mod colour;
mod composite;
mod contour;
mod cycle;
mod distort;
mod effect;
mod generate;
mod homography;
mod image;
mod lens;
mod light;
mod list;
mod mask;
mod par;
mod particles;
mod pen;
mod picture;
mod pixels;
mod plan;
mod projected;
mod radial;
mod reads;
mod render;
mod sound;
mod style;
mod total;
mod vector;

pub use accel::{Accelerator, Amount, Resident, Share, Tally};
pub use blend::{BLEND_MODES, BlendMode, BlendSpec};
pub use blur::Edges;
pub use colour::LUMA;
pub use composite::{BoxCover, OPERATIONS, Operation, OperationSpec, SolidComposite};
pub use effect::{
    Apply, Curve, EFFECTS, Effect, EffectContext, EffectSpec, Growth, Input, Move, ParamSpec,
    ParamType, ParamValue, Reach, Space,
};
pub use homography::Homography;
pub use image::Image;
pub use lens::{IRIS_ROWS, Iris, LensBlur, LensMap, Level as LensLevel, Row as IrisRow};
pub use light::Bevel;
pub use list::{
    Blending, Clip, Frame, Glyph, GlyphDraw, ImageDraw, Layer, LayerMask, MaskMode, Matte,
    MatteMode, Node, PathDraw, Projection, Shared,
};
pub use mask::SIGMA_PER_FEATHER;
pub use picture::Picture;
pub use pixels::{Pixel, Pixels};
pub use plan::{Blur, Glow, GlowComposite, PLANS, Passes, Plan, PlanSpec, Shadow};
pub use radial::{
    Fade as RadialFade, RadialBlur, RadialKind, Rays, Ring, Rings, Stretch as RadialStretch,
};
pub use render::{Quality, RenderOptions, Simd, render, render_on, render_with};
pub use sound::{Heard, level};
pub use style::{
    CAPS, Cap, Color, Dashes, DrawStyle, FILL_RULES, FillRule, Gradient, GradientShape, JOINS,
    Join, Paint, Stop, StrokeStyle,
};
pub use total::{Total, TotalPiece};
pub use vector::{Level as VectorLevel, Streak as VectorStreak, VectorBlur};
/// The geometry and font types display lists are made of: vocabulary crates
/// the rasteriser shares, named from themselves so it stays private ([D37]).
///
/// [D37]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d37-engines-stay-behind-our-own-types
pub use {kurbo, peniko};
