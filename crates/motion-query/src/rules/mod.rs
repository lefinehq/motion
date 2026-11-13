//! The rules `check` runs on every node at every frame.
//!
//! A side branch: works on the output of step 3, not a numbered
//! rendering step. A rule answers one question about one node at one frame:
//! "by how many pixels is this wrong, and how far must it move?". It does not
//! look at other frames or merge anything; `check` does the sampling, merges
//! consecutive frames into one line and builds the fix hint.
//!
//! # Measured axes
//!
//! A bounds rule measures an axis only if the box is no larger than the
//! rule's area on that axis (the canvas for `offscreen`, the 90% safe area
//! for `unsafe`). A larger box is deliberate bleed, such as a full-bleed
//! background; it is not reported on that axis, and if no axis is measured
//! the rule reports nothing for that node. Fix hints clamp the box centre
//! per axis, so one edit fixes a corner overshoot and applying it makes the
//! rule pass.
//!
//! # Adding a rule
//!
//! 1. Create `rules/<name>.rs` with a `pub(super) const RULE: Rule` and a
//!    function `fn(&NodeState, &SceneGraph) -> Option<Overshoot>` that
//!    returns `None` when the node is fine on that frame.
//! 2. Declare the module below and add `<name>::RULE` to [`RULES`]. Order
//!    matters: it is the order of lines in the report.
//! 3. If the rule would only repeat a bigger problem, set `suppressed_by` to
//!    the code of that rule so the report stays short. That rule must be
//!    listed EARLIER in [`RULES`], because `check` only knows about rules
//!    that already ran for the node.
//! 4. Add unit tests next to the rule and a scene test in `check.rs`.

mod bounds;

use crate::report::Severity;
use motion_eval::NodeState;
use motion_scene::SceneGraph;

/// A rule's finding on one frame: how far, and how to move back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Overshoot {
    /// The largest overshoot over the measured axes, in pixels.
    pub amount: f64,
    /// The move that brings the box back, in whole pixels where possible:
    /// `(dx, dy)`, with 0 on an axis that does not need to move.
    pub shift: (f64, f64),
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

/// An axis-aligned rectangle as `(left, top, right, bottom)`.
pub(super) type Area = (f64, f64, f64, f64);

/// Measures `node`'s box against `area` and says how to move it back.
///
/// An axis on which the box is larger than `area` is **not measured**: that
/// is deliberate bleed (a full-bleed background), and no move could satisfy
/// it. If neither axis is measured, or nothing overshoots by more than
/// [`EPSILON`], the result is `None`.
///
/// The shift clamps the box centre into the range that keeps the whole box
/// inside `aim`, on every axis that overshoots (both at once for a corner).
/// An axis whose box does not fit `aim` falls back to `area`, which it does
/// fit, because it was measured.
pub(super) fn overshoot(node: &NodeState, area: Area, aim: Area) -> Option<Overshoot> {
    let x = axis(node.x, node.width, (area.0, area.2), (aim.0, aim.2));
    let y = axis(node.y, node.height, (area.1, area.3), (aim.1, aim.3));
    let amount = [x, y]
        .into_iter()
        .flatten()
        .map(|a| a.0)
        .max_by(f64::total_cmp)?;
    Some(Overshoot {
        amount,
        shift: (x.map_or(0.0, |a| a.1), y.map_or(0.0, |a| a.1)),
    })
}

/// One axis of [`overshoot`]: `(amount, shift)` or `None` when this axis is
/// unmeasured or inside `area`. `centre` and `size` describe the box.
fn axis(centre: f64, size: f64, area: (f64, f64), aim: (f64, f64)) -> Option<(f64, f64)> {
    // Larger than the area: deliberate bleed, not measured.
    if size > area.1 - area.0 + EPSILON {
        return None;
    }
    let amount = (area.0 - (centre - size / 2.0)).max(centre + size / 2.0 - area.1);
    if amount <= EPSILON {
        return None;
    }
    let aim = if size <= aim.1 - aim.0 + EPSILON {
        aim
    } else {
        area
    };
    let (lo, hi) = (aim.0 + size / 2.0, aim.1 - size / 2.0);
    let target = centre.clamp(lo, hi);
    let exact = target - centre;
    // Whole pixels read better, and ignoring float noise keeps 55.000000000000007
    // at 55. Rounding away from zero can only overshoot the target range by
    // less than a pixel; fall back to the exact value if it would.
    let whole = exact.signum() * (exact.abs() - EPSILON).ceil();
    let rounded_centre = centre + whole;
    let shift = if rounded_centre >= lo - EPSILON && rounded_centre <= hi + EPSILON {
        whole
    } else {
        exact
    };
    Some((amount, shift))
}
