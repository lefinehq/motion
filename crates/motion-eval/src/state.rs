//! The result types of evaluation.
//!
//! Part of step 3 (Evaluate). Plain data in pixels, one [`NodeState`] per
//! node, so checks and renderers never need to know about animations.

/// One node's resolved values at a single frame.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeState {
    /// The layer id this state belongs to.
    pub id: String,
    /// Centre x in pixels, with any entrance offset applied.
    pub x: f64,
    /// Centre y in pixels, with any entrance offset applied.
    pub y: f64,
    /// Box width in pixels.
    pub width: f64,
    /// Box height in pixels.
    pub height: f64,
    /// Opacity from 0 (invisible) to 1 (fully visible).
    pub opacity: f64,
}

impl NodeState {
    /// The x coordinate of the left edge.
    pub fn left(&self) -> f64 {
        self.x - self.width / 2.0
    }

    /// The x coordinate of the right edge.
    pub fn right(&self) -> f64 {
        self.x + self.width / 2.0
    }

    /// The y coordinate of the top edge.
    pub fn top(&self) -> f64 {
        self.y - self.height / 2.0
    }

    /// The y coordinate of the bottom edge.
    pub fn bottom(&self) -> f64 {
        self.y + self.height / 2.0
    }
}

/// The state of the whole scene at one point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameState {
    /// The time this state was evaluated at, in frames.
    pub frame: f64,
    /// One state per node, in the graph's paint order.
    pub nodes: Vec<NodeState>,
}
