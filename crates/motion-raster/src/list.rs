//! The display list: one frame as plain data, a tree of [`Layer`]s whose
//! leaves are paths and glyph runs. It knows nothing of scenes or animation;
//! whatever evaluates a scene at a time writes one of these, and
//! [`render`](crate::render) draws it.

use vello_cpu::kurbo::{Affine, BezPath, Rect, Shape};
use vello_cpu::peniko::FontData;

use crate::blend::BlendMode;
use crate::effect::Effect;
use crate::style::{Color, DrawStyle, FillRule, Paint};

/// One frame to draw.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame {
    /// The colour behind everything; transparent when `None`.
    pub background: Option<Color>,
    /// The colour space layers blend in.
    pub blending: Blending,
    /// The top-level nodes, back to front.
    pub nodes: Vec<Node>,
}

/// The colour space in which layers blend and colours mix ([D21], [D34]).
///
/// [D21]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d21-linear-light-float-compositing
/// [D34]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blending {
    /// On gamma-encoded sRGB values, as projects that do not linearise blend.
    #[default]
    Gamma,
    /// On linear-light values. Not drawable yet: the rasteriser blends only
    /// encoded values, so [`render`](crate::render) reports it.
    Linear,
}

/// One node of the tree: a layer with children, or a leaf.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A group with its own transform, opacity, blend mode, clip and effects.
    Layer(Layer),
    /// An outline, filled or stroked.
    Path(PathDraw),
    /// A run of glyphs from one font, filled or stroked.
    Glyphs(GlyphDraw),
}

/// A group of nodes drawn together.
///
/// A layer whose opacity is 1, whose blend mode is normal, which has no clip
/// and no effects, and which is not marked `isolate` only moves its
/// children: it costs no extra buffer. Otherwise it is isolated: its
/// children are drawn into a buffer of their own, so their blend modes see
/// only each other, its effects run on that buffer in order, and the result
/// is clipped and blended onto what is below with its opacity.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// From the layer's coordinates to its parent's.
    pub transform: Affine,
    /// How opaque the layer is, 0 to 1.
    pub opacity: f32,
    /// How it combines with what is below.
    pub blend: BlendMode,
    /// The region outside which nothing of it shows, in its own coordinates.
    pub clip: Option<Clip>,
    /// Effects run on its drawn content, in order.
    pub effects: Vec<Effect>,
    /// Its nodes, back to front.
    pub children: Vec<Node>,
    /// Draw it in a buffer of its own even when nothing else asks for one,
    /// as a nested composition is: its children blend with each other, not
    /// with what is below it.
    pub isolate: bool,
}

impl Layer {
    /// A plain layer: no transform, fully opaque, normal blending, holding `children`.
    pub fn new(children: Vec<Node>) -> Layer {
        Layer {
            transform: Affine::IDENTITY,
            opacity: 1.0,
            blend: BlendMode::Normal,
            clip: None,
            effects: Vec::new(),
            children,
            isolate: false,
        }
    }

    /// Whether drawing it needs a buffer of its own.
    pub fn is_isolated(&self) -> bool {
        self.isolate
            || self.opacity < 1.0
            || self.blend != BlendMode::Normal
            || self.clip.is_some()
            || !self.effects.is_empty()
    }
}

/// The region a layer is clipped to.
#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
    /// A rectangle, such as a nested composition's bounds.
    Rect(Rect),
    /// Any outline, with its fill rule.
    Path(BezPath, FillRule),
}

impl Clip {
    /// The clip as an outline and the rule that says what is inside.
    pub(crate) fn outline(&self) -> (BezPath, FillRule) {
        match self {
            Clip::Rect(rect) => (rect.to_path(0.1), FillRule::NonZero),
            Clip::Path(path, rule) => (path.clone(), *rule),
        }
    }
}

/// An outline painted in its layer's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PathDraw {
    /// The outline.
    pub path: BezPath,
    /// What it is painted with.
    pub paint: Paint,
    /// Filled or stroked.
    pub style: DrawStyle,
}

/// One glyph of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// The glyph's id in its font (not a character code).
    pub id: u32,
    /// Its origin on the baseline, in the run's coordinates.
    pub x: f32,
    /// Its baseline, in the run's coordinates.
    pub y: f32,
}

/// Glyphs of one font at one size, painted in their layer's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphDraw {
    /// The font file's data and the index of the face in it.
    pub font: FontData,
    /// The size in pixels per em.
    pub size: f32,
    /// The glyphs, positioned on their baselines.
    pub glyphs: Vec<Glyph>,
    /// Applied to each glyph about its own origin, such as a slant for faux italic.
    pub glyph_transform: Option<Affine>,
    /// A variable font's position on its axes, normalised as the font's
    /// `fvar`/`avar` tables give it; empty for static fonts.
    pub coords: Vec<i16>,
    /// What the glyphs are painted with.
    pub paint: Paint,
    /// Filled or stroked.
    pub style: DrawStyle,
}

#[cfg(test)]
#[path = "../tests/unit/list.rs"]
mod tests;
