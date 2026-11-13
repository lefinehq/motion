//! The rules `check` runs on every node at every frame.
//!
//! Part of step 4 (Query). A rule answers one question about one node at one
//! frame: "by how many pixels is this wrong, and on which edge?". It does not
//! look at other frames or merge anything; `check` does the sampling, merges
//! consecutive frames into one line and builds the fix hint.
//!
//! # Adding a rule
//!
//! 1. Create `rules/<name>.rs` with a `pub(super) const RULE: Rule` and a
//!    function `fn(&NodeState, &SceneGraph) -> Option<Overshoot>` that
//!    returns `None` when the node is fine on that frame.
//! 2. Declare the module below and add `<name>::RULE` to [`RULES`]. Order
//!    matters: it is the order of lines in the report.
//! 3. If the rule would only repeat a bigger problem, set `suppressed_by` to
//!    the code of that rule so the report stays short.
//! 4. Add unit tests next to the rule and a scene test in `check.rs`.

mod bounds;

use crate::report::Severity;
use motion_eval::NodeState;
use motion_scene::SceneGraph;

/// Which side of a box overshoots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    /// The left edge is too far left.
    Left,
    /// The right edge is too far right.
    Right,
    /// The top edge is too high.
    Top,
    /// The bottom edge is too low.
    Bottom,
}

/// A rule's finding on one frame: how far, and on which edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Overshoot {
    /// The largest overshoot, in pixels.
    pub amount: f64,
    /// The edge it happens on.
    pub edge: Edge,
}

/// A rule that can be run on a node at one frame.
pub(crate) struct Rule {
    /// The short name shown in reports.
    pub code: &'static str,
    /// How serious a violation of this rule is.
    pub severity: Severity,
    /// If a node has a violation with this code, this rule is skipped for it.
    pub suppressed_by: Option<&'static str>,
    /// The test itself.
    pub evaluate: fn(&NodeState, &SceneGraph) -> Option<Overshoot>,
}

/// Every rule, in report order.
pub(crate) const RULES: &[Rule] = &[bounds::OFFSCREEN, bounds::UNSAFE];

/// Floating-point noise below this many pixels is not an overshoot.
///
/// Without it, an entrance at 11/12 of 60 px is 55.000000000000007 and would
/// round up to 56 instead of 55.
pub(crate) const EPSILON: f64 = 1e-9;

/// The largest overshoot of `node`'s box outside the rectangle
/// `left..right` x `top..bottom`, if any edge is beyond it by more than
/// [`EPSILON`].
pub(super) fn overshoot(
    node: &NodeState,
    (left, top, right, bottom): (f64, f64, f64, f64),
) -> Option<Overshoot> {
    [
        (left - node.left(), Edge::Left),
        (node.right() - right, Edge::Right),
        (top - node.top(), Edge::Top),
        (node.bottom() - bottom, Edge::Bottom),
    ]
    .into_iter()
    .filter(|(amount, _)| *amount > EPSILON)
    // `max_by` keeps the last of equal values; ties do not matter here.
    .max_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(amount, edge)| Overshoot { amount, edge })
}
