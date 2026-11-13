//! The scene graph types: the output of compilation.
//!
//! Part of step 2 (Compile). Everything here is plain data in pixels and
//! frames; nothing needs parsing or unit conversion any more.

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
    /// One track per property, parallel to `motion_format::PROPERTIES`: the
    /// length must equal the registry's, which evaluation relies on.
    pub tracks: Vec<Track>,
    /// Whether `(x, y)` still follow the layer's `at`. False for an axis that
    /// an `anim` track drives or whose entrance starts at an absolute value,
    /// because moving `at` then no longer moves the whole motion.
    pub follows_at: (bool, bool),
}
