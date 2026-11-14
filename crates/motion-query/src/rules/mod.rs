//! The rules `check` runs: each says by how many pixels one node is wrong at one
//! frame; `check` does the sampling, merging and fix hints.
//!
//! # Adding a rule
//!
//! 1. Add a [`Code`] variant and its name in `report.rs`.
//! 2. Write `rules/<name>.rs` with a `pub(super) const RULE: Rule`.
//! 3. Add it to [`RULES`], whose order is the report's order.
//! 4. If it would only repeat a bigger problem, set `suppressed_by` to a rule listed earlier.
//! 5. Add unit tests for the rule and a scene test in `check.rs`.

mod bounds;

use crate::report::{Code, Severity};
use motion_format::property::Prop;
use motion_scene::NodeState;
use motion_scene::SceneGraph;

/// A rule's finding on one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Overshoot {
    /// The largest overshoot over the measured axes, in pixels.
    pub amount: f64,
}

/// A rule that can be run on a node at one frame.
pub(crate) struct Rule {
    /// The rule's identity.
    pub code: Code,
    /// How serious a violation of this rule is.
    pub severity: Severity,
    /// A rule whose failure on a node makes this one skip that node.
    pub suppressed_by: Option<Code>,
    /// The finding for a node at one frame, or `None` when it is fine.
    pub evaluate: fn(&NodeState, &SceneGraph) -> Option<Overshoot>,
    /// `(area, aim)`: the rule's area and the one its fix hint aims at, which may be
    /// stricter so one edit clears later rules too.
    pub areas: fn(&SceneGraph) -> (Area, Area),
}

/// Every rule, in report order.
pub(crate) const RULES: &[Rule] = &[bounds::OFFSCREEN, bounds::UNSAFE];

/// Float noise below this many pixels is not an overshoot.
pub(crate) const EPSILON: f64 = 1e-9;

/// An opacity at or below this is invisible, and an invisible node may sit anywhere.
const INVISIBLE: f64 = 0.0;

/// Whether rules look at `node` on this frame.
pub(crate) fn visible(node: &NodeState) -> bool {
    node.get(Prop::OPACITY) > INVISIBLE
}

/// An axis-aligned rectangle as `(left, top, right, bottom)`.
pub(crate) type Area = (f64, f64, f64, f64);

/// How far `node`'s drawn box leaves `area`. An axis on which the box is larger is
/// deliberate bleed (a full-bleed background), so it is not measured.
pub(super) fn overshoot(node: &NodeState, area: Area) -> Option<Overshoot> {
    let (left, top, right, bottom) = node.bounds();
    let x = axis_amount(left, right, (area.0, area.2));
    let y = axis_amount(top, bottom, (area.1, area.3));
    let amount = [x, y].into_iter().flatten().max_by(f64::total_cmp)?;
    Some(Overshoot { amount })
}

/// One axis of [`overshoot`] for the box `lo..hi`.
fn axis_amount(lo: f64, hi: f64, area: (f64, f64)) -> Option<f64> {
    if hi - lo > area.1 - area.0 + EPSILON {
        return None;
    }
    let amount = (area.0 - lo).max(hi - area.1);
    (amount > EPSILON).then_some(amount)
}

/// The move `(dx, dy)` that clamps `sweep`, the box a node covers over its visible
/// frames, into `aim`, else into `area`, else nowhere, axis by axis.
pub(crate) fn shift(sweep: Area, area: Area, aim: Area) -> (f64, f64) {
    (
        axis_shift((sweep.0, sweep.2), (area.0, area.2), (aim.0, aim.2)),
        axis_shift((sweep.1, sweep.3), (area.1, area.3), (aim.1, aim.3)),
    )
}

/// One axis of [`shift`]. All pairs are `(low edge, high edge)`.
fn axis_shift(sweep: (f64, f64), area: (f64, f64), aim: (f64, f64)) -> f64 {
    let fits = |r: (f64, f64)| sweep.1 - sweep.0 <= r.1 - r.0 + EPSILON;
    let Some(target) = [aim, area].into_iter().find(|&r| fits(r)) else {
        return 0.0;
    };
    let exact = if sweep.0 < target.0 - EPSILON {
        target.0 - sweep.0
    } else if sweep.1 > target.1 + EPSILON {
        target.1 - sweep.1
    } else {
        return 0.0;
    };
    // Whole pixels read better, unless rounding pushes the far edge out of the target.
    let whole = exact.signum() * (exact.abs() - EPSILON).ceil();
    let inside = sweep.0 + whole >= target.0 - EPSILON && sweep.1 + whole <= target.1 + EPSILON;
    if inside { whole } else { exact }
}

#[cfg(test)]
#[path = "../../tests/unit/rules/mod.rs"]
mod tests;
