//! The result of evaluation: plain values, so checks and renderers never see animation.

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
    /// One value per property, parallel to `motion_format::property::PROPERTIES`.
    pub values: Vec<f64>,
}

impl NodeState {
    /// The value of `prop`.
    pub fn get(&self, prop: Prop) -> f64 {
        self.values[prop.index()]
    }

    /// The axis-aligned box `(left, top, right, bottom)` as drawn: scaled, then
    /// rotated by `rotate` degrees about `(x, y)`.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let (x, y) = (self.get(Prop::X), self.get(Prop::Y));
        // Mirroring (a negative scale) does not change the covered area.
        let scale = self.get(Prop::SCALE).abs();
        let (half_w, half_h) = (self.width * scale / 2.0, self.height * scale / 2.0);
        let theta = self.get(Prop::ROTATE).to_radians();
        // Snap trig dust (cos 90° is 6e-17) so a quarter turn gives exact edges.
        let snap = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v.abs() };
        let (cos, sin) = (snap(theta.cos()), snap(theta.sin()));
        let half_x = half_w * cos + half_h * sin;
        let half_y = half_w * sin + half_h * cos;
        (x - half_x, y - half_y, x + half_x, y + half_y)
    }

    /// The left edge of [`NodeState::bounds`].
    pub fn left(&self) -> f64 {
        self.bounds().0
    }

    /// The right edge of [`NodeState::bounds`].
    pub fn right(&self) -> f64 {
        self.bounds().2
    }

    /// The top edge of [`NodeState::bounds`].
    pub fn top(&self) -> f64 {
        self.bounds().1
    }

    /// The bottom edge of [`NodeState::bounds`].
    pub fn bottom(&self) -> f64 {
        self.bounds().3
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
