//! The result of evaluation: plain values, so checks and renderers never see animation.

use kurbo::{Affine, Rect};
use motion_format::Value;
use motion_format::property::Prop;

/// One node's resolved values at a single frame.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeState {
    /// The layer id this state belongs to.
    pub id: String,
    /// Box width in pixels, in the node's own coordinates.
    pub width: f64,
    /// Box height in pixels, in the node's own coordinates.
    pub height: f64,
    /// One value per property, parallel to `motion_format::property::PROPERTIES`.
    pub values: Vec<Value>,
    /// From the node's own coordinates to the canvas: its transform, then its parents'.
    pub transform: Affine,
    /// Whether the frame falls in the node's span.
    pub shown: bool,
}

impl NodeState {
    /// The value of `prop`.
    pub fn get(&self, prop: Prop) -> Value {
        self.values[prop.index()]
    }

    /// The value of a number property.
    ///
    /// # Panics
    /// If `prop` does not hold a number.
    pub fn number(&self, prop: Prop) -> f64 {
        self.get(prop).as_number().expect("a number property")
    }

    /// The value of a point property.
    ///
    /// # Panics
    /// If `prop` does not hold a point.
    pub fn point(&self, prop: Prop) -> [f64; 2] {
        self.get(prop).as_point().expect("a point property")
    }

    /// The axis-aligned box the node covers on the canvas: its own box
    /// through its transform.
    pub fn bounds(&self) -> Rect {
        let drawn =
            self.transform
                .transform_rect_bbox(Rect::new(0.0, 0.0, self.width, self.height));
        // Snap trig dust (cos 90° is 6e-17) so a quarter turn gives exact edges.
        let snap = |v: f64| {
            let near = v.round();
            if (v - near).abs() < 1e-9 { near } else { v }
        };
        Rect::new(
            snap(drawn.x0),
            snap(drawn.y0),
            snap(drawn.x1),
            snap(drawn.y1),
        )
    }

    /// The left edge of [`NodeState::bounds`].
    pub fn left(&self) -> f64 {
        self.bounds().x0
    }

    /// The right edge of [`NodeState::bounds`].
    pub fn right(&self) -> f64 {
        self.bounds().x1
    }

    /// The top edge of [`NodeState::bounds`].
    pub fn top(&self) -> f64 {
        self.bounds().y0
    }

    /// The bottom edge of [`NodeState::bounds`].
    pub fn bottom(&self) -> f64 {
        self.bounds().y1
    }
}

/// The state of the whole scene at one point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameState {
    /// The time evaluated, in frames.
    pub frame: f64,
    /// One state per node, in the graph's paint order.
    pub nodes: Vec<NodeState>,
}

#[cfg(test)]
#[path = "../tests/unit/state.rs"]
mod tests;
