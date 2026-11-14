//! The scene graph: the compiler's output, plain data in pixels and frames.

use motion_format::Value;

use crate::track::Track;

/// A compiled scene: canvas, timing and nodes in paint order.
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
}

/// The kind of thing a node draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A line of text.
    Text,
    /// A rectangle.
    Rect,
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
    pub fn sample(&self, frame: f64, ty: motion_format::ValueType) -> Value {
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
    pub kind: NodeKind,
    /// Box width in pixels, in the node's own coordinates.
    pub width: f64,
    /// Box height in pixels, in the node's own coordinates.
    pub height: f64,
    /// The index of the node it moves with, if any.
    pub parent: Option<usize>,
    /// When it shows and how its own time runs.
    pub timing: Timing,
    /// One track per property, parallel to `motion_format::property::PROPERTIES`,
    /// sampled at the node's own frame.
    pub tracks: Vec<PropertyTrack>,
    /// Whether moving the layer's `at` moves the whole motion on `(x, y)`.
    pub follows_at: (bool, bool),
}

#[cfg(test)]
#[path = "../tests/unit/graph.rs"]
mod tests;
