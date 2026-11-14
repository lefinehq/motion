//! The scene graph: the compiler's output, plain data in pixels and frames.

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

/// One drawable thing with a resolved box and a track per property.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The layer id this node came from.
    pub id: String,
    /// What the node draws.
    pub kind: NodeKind,
    /// Box width in pixels.
    pub width: f64,
    /// Box height in pixels.
    pub height: f64,
    /// One track per property, parallel to `motion_format::property::PROPERTIES`.
    pub tracks: Vec<Track>,
    /// Whether moving the layer's `at` moves the whole motion on `(x, y)`.
    pub follows_at: (bool, bool),
}
