//! The result types of evaluation.
//!
//! Part of step 3 (Evaluate). Plain data in pixels, one [`NodeState`] per
//! node, so checks and renderers never need to know about animations.

use motion_format::property::Prop;

/// One node's resolved values at a single frame.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeState {
    /// The layer id this state belongs to.
    pub id: String,
    /// Box width in pixels, before `scale` and `rotate`.
    pub width: f64,
    /// Box height in pixels, before `scale` and `rotate`.
    pub height: f64,
    /// One value per property, parallel to
    /// `motion_format::property::PROPERTIES` and clamped to each property's
    /// range. Read them with [`NodeState::get`].
    pub values: Vec<f64>,
}

impl NodeState {
    /// The value of `prop`.
    pub fn get(&self, prop: Prop) -> f64 {
        self.values[prop.index()]
    }

    /// The axis-aligned bounding box `(left, top, right, bottom)` of the
    /// box as drawn: `width x |scale|` by `height x |scale|`, rotated by
    /// `rotate` degrees around the centre `(x, y)`.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let (x, y) = (self.get(Prop::X), self.get(Prop::Y));
        // Mirroring (a negative scale) does not change the covered area.
        let scale = self.get(Prop::SCALE).abs();
        let (half_w, half_h) = (self.width * scale / 2.0, self.height * scale / 2.0);
        let theta = self.get(Prop::ROTATE).to_radians();
        // At 90 degrees cos is 6e-17, not 0; snap such dust so a quarter turn
        // gives exact edges instead of fractions of a pixel.
        let snap = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v.abs() };
        let (cos, sin) = (snap(theta.cos()), snap(theta.sin()));
        let half_x = half_w * cos + half_h * sin;
        let half_y = half_w * sin + half_h * cos;
        (x - half_x, y - half_y, x + half_x, y + half_y)
    }

    /// The x coordinate of the left edge of [`NodeState::bounds`].
    pub fn left(&self) -> f64 {
        self.bounds().0
    }

    /// The x coordinate of the right edge of [`NodeState::bounds`].
    pub fn right(&self) -> f64 {
        self.bounds().2
    }

    /// The y coordinate of the top edge of [`NodeState::bounds`].
    pub fn top(&self) -> f64 {
        self.bounds().1
    }

    /// The y coordinate of the bottom edge of [`NodeState::bounds`].
    pub fn bottom(&self) -> f64 {
        self.bounds().3
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

#[cfg(test)]
#[path = "../tests/unit/state.rs"]
mod tests;
