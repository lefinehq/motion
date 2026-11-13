//! The scene graph types: the output of compilation.
//!
//! Part of step 2 (Compile). Everything here is plain data in pixels and
//! frames; nothing needs parsing or unit conversion any more.

use motion_format::shorthand::Easing;

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

/// One drawable thing with a resolved box.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The layer id this node came from.
    pub id: String,
    /// What the node draws.
    pub kind: NodeKind,
    /// Centre x in pixels.
    pub x: f64,
    /// Centre y in pixels.
    pub y: f64,
    /// Box width in pixels.
    pub width: f64,
    /// Box height in pixels.
    pub height: f64,
    /// Entrance animation, if the layer has one.
    pub enter: Option<Enter>,
}

/// An entrance animation, with all times in frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enter {
    /// Frame at which the entrance begins.
    pub start: f64,
    /// Length of the entrance in frames.
    pub duration: f64,
    /// Easing applied to the entrance's progress.
    pub easing: Easing,
    /// Where the node starts, relative to its final position, in pixels.
    pub from_offset: (f64, f64),
}
