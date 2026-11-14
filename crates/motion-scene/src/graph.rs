//! The scene graph: the compiler's output, plain data in pixels and frames.

use motion_format::item::ItemKind;
use motion_format::property::Prop;
use std::sync::Arc;

use kurbo::Rect;
use motion_format::shorthand::RectShape;

use crate::text::TextNode;
use motion_format::{Value, ValueType};

use crate::track::Track;

/// A compiled scene: canvas, timing and nodes in paint order, and the
/// compositions its `comp` nodes draw.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneGraph {
    /// Canvas width in pixels.
    pub width: f64,
    /// Canvas height in pixels.
    pub height: f64,
    /// Frames per second.
    pub fps: f64,
    /// Scene length in frames.
    pub duration_frames: f64,
    /// Nodes in paint order: the first is at the back.
    pub nodes: Vec<Node>,
    /// Every composition, in the scene's order; [`NodeContent::Comp`] indexes it.
    pub comps: Vec<CompGraph>,
}

/// A compiled composition: its own canvas, clock and nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct CompGraph {
    /// Its id in the scene.
    pub id: String,
    /// Canvas width in pixels.
    pub width: f64,
    /// Canvas height in pixels.
    pub height: f64,
    /// Frames per second of its own timeline.
    pub fps: f64,
    /// Length in its own frames; a node drawing it shows nothing past it.
    pub duration_frames: f64,
    /// The colour behind its nodes, straight RGBA from 0 to 1.
    pub background: [f64; 4],
    /// Nodes in paint order: the first is at the back.
    pub nodes: Vec<Node>,
}

/// What a node draws.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeContent {
    /// Text, set, and its animators.
    Text {
        /// The words, their style and their layout.
        text: Arc<TextNode>,
        /// Its animators, in order; hidden ones are left out.
        animators: Vec<ItemNode>,
    },
    /// A rectangle the size of the node's box.
    Rect(RectShape),
    /// Shape items; the first draws on top.
    Shapes(Vec<ItemNode>),
    /// The composition at this index of [`SceneGraph::comps`].
    Comp(usize),
    /// Nothing; other nodes move with it.
    Pivot,
}

/// A compiled shape item or effect: its kind, a track per property of the
/// kind's registry, its settings and a group's items.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemNode {
    /// Its kind.
    pub kind: &'static ItemKind,
    /// Its id, if the scene gave one.
    pub id: Option<String>,
    /// One track per row of the kind's registry, sampled at the node's own frame.
    pub tracks: Vec<PropertyTrack>,
    /// The choice of each of the kind's settings.
    pub settings: Vec<usize>,
    /// A group's items, or an animator's selectors; the first draws on top.
    pub items: Vec<ItemNode>,
    /// The properties it sets or animates, in registry order.
    pub written: Vec<Prop>,
}

/// When a node shows and how its own time runs, in frames of its composition's
/// timeline (parenting moves a layer, not its time).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// The frames it shows in, `[start, end)`; always when `None`.
    pub span: Option<(f64, f64)>,
    /// Where its own frame 0 falls.
    pub offset: f64,
    /// How slowly its own time runs; negative runs backwards from `offset`.
    pub stretch: f64,
}

impl Timing {
    /// Always shown, its own time the same as its parent's.
    pub const ALWAYS: Timing = Timing {
        span: None,
        offset: 0.0,
        stretch: 1.0,
    };

    /// The node's own frame at the composition's `frame`.
    pub fn local(&self, frame: f64) -> f64 {
        (frame - self.offset) / self.stretch
    }

    /// Whether it shows at the composition's `frame`.
    pub fn shows_at(&self, frame: f64) -> bool {
        self.span
            .is_none_or(|(start, end)| frame >= start && frame < end)
    }
}

/// How one property moves: as a whole, or one component per track.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyTrack {
    /// One track of the property's type.
    Whole(Track),
    /// One number track per component.
    Split(Vec<Track>),
}

impl PropertyTrack {
    /// The value at `frame`.
    pub fn sample(&self, frame: f64, ty: ValueType) -> Value {
        match self {
            PropertyTrack::Whole(track) => track.sample(frame),
            PropertyTrack::Split(tracks) => {
                let components: Vec<f64> = tracks
                    .iter()
                    .map(|t| t.sample(frame).components()[0])
                    .collect();
                Value::from_components(ty, &components)
            }
        }
    }

    /// Whether the value is the same at every frame.
    pub fn is_constant(&self) -> bool {
        match self {
            PropertyTrack::Whole(track) => track.is_constant(),
            PropertyTrack::Split(tracks) => tracks.iter().all(Track::is_constant),
        }
    }
}

/// One drawable thing with a resolved box and a track per property.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The layer id this node came from.
    pub id: String,
    /// What the node draws.
    pub content: NodeContent,
    /// The colour of a rect node, straight RGBA from 0 to 1.
    pub fill: [f64; 4],
    /// Its box in its own coordinates: a rect's or a composition's from the
    /// origin, a text layout's bounds, or empty at the origin for shapes and pivots.
    pub bounds: Rect,
    /// The index of the node it moves with, if any.
    pub parent: Option<usize>,
    /// When it shows and how its own time runs.
    pub timing: Timing,
    /// One track per property, parallel to `motion_format::property::PROPERTIES`,
    /// sampled at the node's own frame.
    pub tracks: Vec<PropertyTrack>,
    /// Whether moving the layer's `at` moves the whole motion on `(x, y)`.
    pub follows_at: (bool, bool),
    /// Effects on what it draws, in order; hidden ones are left out.
    pub fx: Vec<ItemNode>,
    /// Left out of the frame; its children still move with it.
    pub hidden: bool,
}

#[cfg(test)]
#[path = "../tests/unit/graph.rs"]
mod tests;
