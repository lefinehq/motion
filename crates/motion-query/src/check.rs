//! The `check` query: run every rule over every frame of a scene.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step). Frames are sampled at whole numbers, each rule is
//! asked about each node, and consecutive failing frames become one
//! [`Violation`] so the report stays short. Fix hints come from the box a
//! node sweeps over all its visible frames (see the `rules` module docs).

use crate::report::{Code, Report, Violation};
use crate::rules::{Area, Overshoot, RULES, Rule, shift, visible};
use motion_eval::{FrameState, evaluate};
use motion_format::shorthand::{Coord, Position};
use motion_format::{Layer, Scene};
use motion_scene::SceneGraph;
use tracing::{Span, field, instrument};

/// Runs the standard rules on `scene` over every frame of `graph`.
///
/// `graph` must be the compiled form of `scene`: nodes and layers are matched
/// by position.
#[instrument(
    level = "debug",
    skip_all,
    fields(
        frames = field::Empty,
        rules_run = field::Empty,
        violations = field::Empty,
        passed = field::Empty
    )
)]
pub fn check(scene: &Scene, graph: &SceneGraph) -> Report {
    // Whole frames 0..ceil(duration). Evaluate each once and share it.
    let frame_count = graph.duration_frames.ceil().max(0.0) as u32;
    let frames: Vec<FrameState> = (0..frame_count)
        .map(|f| evaluate(graph, f as f64))
        .collect();
    Span::current().record("frames", frame_count);

    let mut report = Report {
        violations: Vec::new(),
        passed: 0,
    };
    let mut rules_run = 0;
    for (index, layer) in scene.layers.iter().enumerate() {
        rules_run += check_layer(index, layer, &frames, graph, &mut report);
    }
    Span::current()
        .record("rules_run", rules_run)
        .record("violations", report.violations.len())
        .record("passed", report.passed);
    report
}

/// Runs every rule on layer `index`, adding what it finds to `report`, and
/// returns how many rules ran. A rule is skipped when the rule that
/// suppresses it has already failed on this layer.
fn check_layer(
    index: usize,
    layer: &Layer,
    frames: &[FrameState],
    graph: &SceneGraph,
    report: &mut Report,
) -> usize {
    let mut failed: Vec<Code> = Vec::new();
    let mut rules_run = 0;
    for rule in RULES {
        if rule.suppressed_by.is_some_and(|by| failed.contains(&by)) {
            continue;
        }
        rules_run += 1;
        let found = runs(rule, index, frames, graph);
        if found.is_empty() {
            report.passed += 1;
            continue;
        }
        failed.push(rule.code);
        let fix = fix_hint(rule, index, layer, frames, graph);
        for run in found {
            let violation = run.into_violation(rule, &layer.id, fix.clone());
            report.violations.push(violation);
        }
    }
    rules_run
}

/// The edit that clears `rule` for layer `index`, if a move of `at` can:
/// the shift that brings the node's sweep into the rule's aim, on the axes
/// that still follow `at`.
fn fix_hint(
    rule: &Rule,
    index: usize,
    layer: &Layer,
    frames: &[FrameState],
    graph: &SceneGraph,
) -> Option<String> {
    let sweep = sweep(index, frames)?;
    let (area, aim) = (rule.areas)(graph);
    let (dx, dy) = shift(sweep, area, aim);
    // An axis driven by an `anim` track ignores `at`, so moving `at`
    // cannot fix it: leave that axis alone.
    let (follows_x, follows_y) = graph.nodes[index].follows_at;
    let only_followed = |delta: f64, follows: bool| if follows { delta } else { 0.0 };
    fix(
        &layer.id,
        layer.at,
        (only_followed(dx, follows_x), only_followed(dy, follows_y)),
        graph,
    )
}

/// The box node `index` covers over all frames on which it is visible, or
/// `None` if it is never visible.
///
/// A fix moves the resting position, and so every frame at once; clamping
/// this whole sweep (not one frame) is what makes a hint for an animated
/// layer settle in one edit instead of trading one bad frame for another.
fn sweep(index: usize, frames: &[FrameState]) -> Option<Area> {
    frames
        .iter()
        .map(|state| &state.nodes[index])
        .filter(|node| visible(node))
        .map(|n| n.bounds())
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

/// A stretch of consecutive failing frames for one (rule, node).
struct Run {
    first: u32,
    last: u32,
    /// The worst overshoot in the stretch.
    worst: Overshoot,
}

/// Finds the failing stretches of `rule` on node `index`.
fn runs(rule: &Rule, index: usize, frames: &[FrameState], graph: &SceneGraph) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for (frame, state) in frames.iter().enumerate() {
        let frame = frame as u32;
        let Some(o) = (rule.evaluate)(&state.nodes[index], graph) else {
            continue;
        };
        match out.last_mut() {
            Some(run) if run.last + 1 == frame => {
                run.last = frame;
                if o.amount > run.worst.amount {
                    run.worst = o;
                }
            }
            _ => out.push(Run {
                first: frame,
                last: frame,
                worst: o,
            }),
        }
    }
    out
}

impl Run {
    fn into_violation(self, rule: &Rule, id: &str, fix: Option<String>) -> Violation {
        // Round up to whole pixels, ignoring float noise: 60 * (11 / 12) is
        // 55.000000000000007 and must read +55, not +56.
        let amount = (self.worst.amount - crate::rules::EPSILON).ceil();
        Violation {
            severity: rule.severity,
            code: rule.code,
            id: id.to_string(),
            first_frame: self.first,
            last_frame: self.last,
            amount_px: amount,
            fix,
        }
    }
}

/// Builds `id at="x y"` from a shift, or `None` if nothing moves.
///
/// An axis that does not move keeps its original coordinate (so `center`
/// stays `center`); a moving axis resolves `center` to pixels first.
fn fix(id: &str, at: Position, (dx, dy): (f64, f64), graph: &SceneGraph) -> Option<String> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    let moved = |c: Coord, delta: f64, extent: f64| match delta {
        0.0 => c,
        d => Coord::Px(c.resolve(extent) + d),
    };
    let (x, y) = (moved(at.x, dx, graph.width), moved(at.y, dy, graph.height));
    Some(format!("{id} at=\"{x} {y}\""))
}

#[cfg(test)]
#[path = "../tests/unit/check.rs"]
mod tests;
