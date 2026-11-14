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
//! the rule reports nothing for that node.
//!
//! # Fix hints
//!
//! A fix moves the layer's resting position, which moves the box on every
//! frame. So the hint is not taken from one frame: [`shift`] gets the box
//! the node sweeps over all its visible frames (entrance included) and
//! clamps that whole sweep into the rule's aim area, on both axes at once.
//! Applying the hint therefore clears the rule in one edit; an axis whose
//! sweep cannot fit is left alone, and if nothing moves there is no hint.
//!
//! # Adding a rule
//!
//! 1. Add the rule's [`Code`] variant and its name in `report.rs`.
//! 2. Create `rules/<name>.rs` with a `pub(super) const RULE: Rule`, a
//!    function `fn(&NodeState, &SceneGraph) -> Option<Overshoot>` that
//!    returns `None` when the node is fine on that frame, and the areas its
//!    fix hint aims at (see [`Rule::areas`]).
//! 3. Declare the module below and add `<name>::RULE` to [`RULES`]. Order
//!    matters: it is the order of lines in the report.
//! 4. If the rule would only repeat a bigger problem, set `suppressed_by` to
//!    the code of that rule so the report stays short. That rule must be
//!    listed EARLIER in [`RULES`], because `check` only knows about rules
//!    that already ran for the node.
//! 5. Add unit tests next to the rule and a scene test in `check.rs`.

mod bounds;

use crate::report::{Code, Severity};
use motion_eval::NodeState;
use motion_format::property::Prop;
use motion_scene::SceneGraph;

/// A rule's finding on one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Overshoot {
    /// The largest overshoot over the measured axes, in pixels.
    pub amount: f64,
}

/// A rule that can be run on a node at one frame.
pub(crate) struct Rule {
    /// Which rule this is; its name is what reports show.
    pub code: Code,
    /// How serious a violation of this rule is.
    pub severity: Severity,
    /// The rule whose violation on a node makes this one skip that node.
    pub suppressed_by: Option<Code>,
    /// The test itself.
    pub evaluate: fn(&NodeState, &SceneGraph) -> Option<Overshoot>,
    /// The rule's area and the area its fix hint aims at, as
    /// `(area, aim)`. The aim may be stricter than the area, so that one
    /// edit clears this rule and the rules after it.
    pub areas: fn(&SceneGraph) -> (Area, Area),
}

/// Every rule, in report order.
pub(crate) const RULES: &[Rule] = &[bounds::OFFSCREEN, bounds::UNSAFE];

/// Floating-point noise below this many pixels is not an overshoot.
///
/// Without it, an entrance at 11/12 of 60 px is 55.000000000000007 and would
/// round up to 56 instead of 55.
pub(crate) const EPSILON: f64 = 1e-9;

/// An opacity at or below this counts as invisible: an invisible node (for
/// example before its entrance) may sit anywhere.
const INVISIBLE: f64 = 0.0;

/// Whether rules look at `node` on this frame.
pub(crate) fn visible(node: &NodeState) -> bool {
    node.get(Prop::OPACITY) > INVISIBLE
}

/// An axis-aligned rectangle as `(left, top, right, bottom)`.
pub(crate) type Area = (f64, f64, f64, f64);

/// Measures `node`'s box against `area` on one frame.
///
/// An axis on which the box is larger than `area` is **not measured**: that
/// is deliberate bleed (a full-bleed background), and no move could satisfy
/// it. If neither axis is measured, or nothing overshoots by more than
/// [`EPSILON`], the result is `None`.
pub(super) fn overshoot(node: &NodeState, area: Area) -> Option<Overshoot> {
    // The box as drawn: scale and rotation included.
    let (left, top, right, bottom) = node.bounds();
    let x = axis_amount(left, right, (area.0, area.2));
    let y = axis_amount(top, bottom, (area.1, area.3));
    let amount = [x, y].into_iter().flatten().max_by(f64::total_cmp)?;
    Some(Overshoot { amount })
}

/// One axis of [`overshoot`]: the amount, or `None` when this axis is
/// unmeasured or inside `area`. `lo..hi` is the box on this axis.
fn axis_amount(lo: f64, hi: f64, area: (f64, f64)) -> Option<f64> {
    // Larger than the area: deliberate bleed, not measured.
    if hi - lo > area.1 - area.0 + EPSILON {
        return None;
    }
    let amount = (area.0 - lo).max(hi - area.1);
    (amount > EPSILON).then_some(amount)
}

/// The move `(dx, dy)` that brings `sweep` (the box a node covers over all
/// its visible frames) inside `aim`, or failing that inside `area`.
///
/// Each axis is handled on its own: a sweep that fits `aim` is clamped into
/// it, one that only fits `area` is clamped into `area`, and one that fits
/// neither does not move. Because the whole sweep is clamped, the move is
/// right for every frame of an entrance, not just the worst one.
pub(crate) fn shift(sweep: Area, area: Area, aim: Area) -> (f64, f64) {
    (
        axis_shift((sweep.0, sweep.2), (area.0, area.2), (aim.0, aim.2)),
        axis_shift((sweep.1, sweep.3), (area.1, area.3), (aim.1, aim.3)),
    )
}

/// One axis of [`shift`]. All pairs are `(low edge, high edge)`.
fn axis_shift(sweep: (f64, f64), area: (f64, f64), aim: (f64, f64)) -> f64 {
    let fits = |r: (f64, f64)| sweep.1 - sweep.0 <= r.1 - r.0 + EPSILON;
    let target = if fits(aim) {
        aim
    } else if fits(area) {
        area
    } else {
        return 0.0;
    };
    let exact = if sweep.0 < target.0 - EPSILON {
        target.0 - sweep.0
    } else if sweep.1 > target.1 + EPSILON {
        target.1 - sweep.1
    } else {
        return 0.0;
    };
    // Whole pixels read better, and ignoring float noise keeps 55.000000000000007
    // at 55. Rounding away from zero can push the far edge out by under a
    // pixel; fall back to the exact value if it would.
    let whole = exact.signum() * (exact.abs() - EPSILON).ceil();
    let inside = sweep.0 + whole >= target.0 - EPSILON && sweep.1 + whole <= target.1 + EPSILON;
    if inside { whole } else { exact }
}

#[cfg(test)]
#[path = "../../tests/unit/rules/mod.rs"]
mod tests;
