//! The rules `check` runs: each says by how many pixels one node is wrong at one
//! frame; `check` does the sampling, merging and fix hints.
//!
//! # Adding a rule
//!
//! 1. Add a [`Code`] variant in `report.rs`; its name is the variant's, in snake case.
//! 2. Write `rules/<name>.rs` with a `pub(super) const RULE: Rule`.
//! 3. Add it to [`RULES`], whose order is the report's order.
//! 4. If it would only repeat a bigger problem, set `suppressed_by` to a rule listed earlier.
//! 5. Add unit tests for the rule and a scene test in `check.rs`.
//!
//! Craft rules ([D59]) read a layer over many frames instead: a step from
//! one frame to the next, or a stretch of frames. They are listed in
//! [`CRAFT`], after the layout rules, and add a [`Code`] the same way.
//!
//! [D59]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d59-craft-lint-measures-motion-from-the-scene-and-reports-numbers

mod bounds;
pub(crate) mod crowd;
mod easing;
mod judder;
mod text_hold;

use crate::report::{Code, Measure, Severity};
use kurbo::{Affine, Point, Rect, Vec2};
use motion_format::{Address, Layer};
use motion_scene::{CompGraph, ContentState, Node, SceneGraph, Visit};

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
    /// The finding for a node at one frame, at any depth and in scene
    /// space, or `None` when it is fine.
    pub evaluate: fn(&Visit<'_>, &SceneGraph) -> Option<Overshoot>,
    /// `(area, aim)`: the rule's area and the one its fix hint aims at, which may be
    /// stricter so one edit clears later rules too.
    pub areas: fn(&SceneGraph) -> (Rect, Rect),
}

/// Every rule, in report order.
pub(crate) const RULES: &[Rule] = &[bounds::OFFSCREEN, bounds::UNSAFE];

/// What a craft rule found: the worst value and the limit it misses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Finding {
    /// What was measured.
    pub value: f64,
    /// The limit it misses.
    pub limit: f64,
}

impl Finding {
    /// How far past its limit it is, the larger the worse.
    pub fn excess(self) -> f64 {
        (self.value - self.limit).abs()
    }
}

/// What craft rules see of one layer at one frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sample {
    /// The frame of the scene.
    pub frame: u32,
    /// Whether rules look at it on this frame ([`visible`]).
    pub shown: bool,
    /// Its opacity times that of the `comp` nodes around it.
    pub opacity: f64,
    /// The corners of its own box on the canvas.
    pub corners: [Point; 4],
    /// Its own transform in its parent: what moves it on its own.
    pub local: Affine,
    /// How many characters it reads, for a text layer.
    pub chars: Option<usize>,
    /// Whether it is drawn with motion blur.
    pub blurred: bool,
}

impl Sample {
    /// How far its corners move from `before` to it, the farthest one, in
    /// px of the canvas.
    pub fn moved_from(&self, before: &Sample) -> f64 {
        self.corners
            .iter()
            .zip(&before.corners)
            .map(|(a, b)| a.distance(*b))
            .fold(0.0, f64::max)
    }

    /// The middle of its own box on the canvas.
    pub fn centre(&self) -> Point {
        let [a, _, c, _] = self.corners;
        a.midpoint(c)
    }

    /// Whether it moves on its own from `before`, not only with what holds it.
    pub fn moves_from(&self, before: &Sample) -> bool {
        self.local
            .as_coeffs()
            .iter()
            .zip(before.local.as_coeffs())
            .any(|(a, b)| (a - b).abs() > EPSILON)
    }
}

/// What a step rule makes of one step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Measured {
    /// The rule does not apply to it.
    Skipped,
    /// It is within the limit.
    Passed,
    /// It misses the limit.
    Found(Finding),
}

/// How a craft rule reads a layer's frames.
pub(crate) enum Reads {
    /// Each frame against the one before it, both shown.
    Step(fn(&Sample, &Sample, &SceneGraph) -> Measured),
    /// Stretches of consecutive frames in which `holds`. The longest is
    /// judged, by its length in frames and its first sample, once the frames
    /// run out: a layer that holds long enough once has held, however brief
    /// its other stretches, such as the slow start of a move.
    Stretch {
        /// Whether the frame belongs to a stretch, given the frame before it
        /// when that one is the previous frame.
        holds: fn(&Sample, Option<&Sample>, &SceneGraph) -> bool,
        /// The finding on a stretch of this many frames, if any.
        judge: fn(u32, &Sample, &SceneGraph) -> Option<Finding>,
    },
    /// Moves: runs of steps in which `speed` is some, each judged once it
    /// ends.
    Moves {
        /// How fast the layer moves on its own over the step, in px of the
        /// canvas per frame, or `None` when it does not.
        speed: fn(&Sample, &Sample) -> Option<f64>,
        /// What a whole move comes to.
        judge: fn(&Move, &SceneGraph) -> Measured,
    },
}

/// One move of a layer: consecutive steps it takes on its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Move {
    /// The frame it starts from, the one before its first step.
    pub first: u32,
    /// The frame its last step ends on.
    pub last: u32,
    /// Its speeds added up, one per step.
    pub total: f64,
    /// Its fastest step.
    pub peak: f64,
}

impl Move {
    /// How many steps it takes.
    pub fn steps(&self) -> u32 {
        self.last - self.first
    }

    /// Its mean speed per step.
    pub fn mean(&self) -> f64 {
        self.total / f64::from(self.steps())
    }
}

/// A craft rule ([D59]): it measures a layer's motion over its frames and
/// says how far past a limit it goes.
///
/// [D59]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d59-craft-lint-measures-motion-from-the-scene-and-reports-numbers
pub(crate) struct Craft {
    /// The rule's identity.
    pub code: Code,
    /// How serious a finding is.
    pub severity: Severity,
    /// How it reads the frames.
    pub reads: Reads,
    /// The shortest run of failing frames that counts, in seconds; shorter
    /// ones pass.
    pub lasting: f64,
    /// How the worst finding of a run prints.
    pub measure: fn(Finding) -> Measure,
    /// The edit that clears its findings on the layer at the address, as the
    /// scene writes it and the node draws it in the composition, when one
    /// edit can.
    pub fix: fn(&Address, Option<&Layer>, &Node, &CompGraph) -> Option<String>,
}

/// Every craft rule, in report order, after [`RULES`].
pub(crate) const CRAFT: &[Craft] = &[judder::JUDDER, easing::EASING, text_hold::TEXT_HOLD];

/// The most a layer may move in one frame, in px of the canvas, before the
/// eye sees it jump: 11 px on a 1080-px-tall canvas, scaled by its height.
pub(crate) fn judder_limit(graph: &SceneGraph) -> f64 {
    11.0 * px(graph)
}

/// One px of a 1080-px-tall canvas, in px of this one.
pub(crate) fn px(graph: &SceneGraph) -> f64 {
    graph.root.height / 1080.0
}

/// Float noise below this many pixels is not an overshoot.
pub(crate) const EPSILON: f64 = 1e-9;

/// An opacity at or below this is invisible, and an invisible node may sit anywhere.
const INVISIBLE: f64 = 0.0;

/// Whether rules look at a node on this frame: it and the `comp` nodes
/// around it show, together they are not see-through, and they do not clip
/// all of it away.
pub(crate) fn visible(visit: &Visit<'_>) -> bool {
    visit.visible && visit.opacity > INVISIBLE && visit.drawn().is_some()
}

/// Whether a node draws anything of its own: a pivot or a sound draws
/// nothing, so it may sit anywhere.
pub(crate) fn draws(visit: &Visit<'_>) -> bool {
    !matches!(
        visit.node.content,
        ContentState::Pivot | ContentState::Audio
    )
}

/// How far the part of a node that draws leaves `area`. An axis on which
/// that part is larger is deliberate bleed (a full-bleed background), so it
/// is not measured.
pub(super) fn overshoot(visit: &Visit<'_>, area: Rect) -> Option<Overshoot> {
    let b = visit.drawn()?;
    let x = axis_amount(b.x0, b.x1, (area.x0, area.x1));
    let y = axis_amount(b.y0, b.y1, (area.y0, area.y1));
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

/// The move that clamps `sweep`, the box a node covers over its visible frames,
/// into `aim`, else into `area`, else nowhere, axis by axis.
pub(crate) fn shift(sweep: Rect, area: Rect, aim: Rect) -> Vec2 {
    Vec2::new(
        axis_shift((sweep.x0, sweep.x1), (area.x0, area.x1), (aim.x0, aim.x1)),
        axis_shift((sweep.y0, sweep.y1), (area.y0, area.y1), (aim.y0, aim.y1)),
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
