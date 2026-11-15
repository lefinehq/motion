//! The `check` query: every rule over every frame, with consecutive failing frames
//! merged into one [`Violation`] so the report stays short.

use crate::report::{Code, Report, Violation};
use crate::rules::{EPSILON, Overshoot, RULES, Rule, shift, visible};
use kurbo::{Affine, Point, Rect, Vec2};
use motion_format::key::AT;
use motion_format::shorthand::{Coord, Position};
use motion_format::{Address, Layer, Scene};
use motion_scene::{
    CompGraph, Node, SceneGraph, Visit, address_of, evaluate, local_transform, walk,
};
use tracing::{Span, field, instrument};

/// Runs the standard rules on `scene` over every frame of `graph`, its
/// compiled form, on every node at any depth, in scene space.
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
    debug_assert_eq!(scene.root.layers.len(), graph.root.nodes.len());
    let frame_count = graph.root.duration_frames.ceil().max(0.0) as u32;
    Span::current().record("frames", frame_count);

    // One frame at a time: memory grows with layers and rules, not frames.
    let paths = graph.paths();
    let mut scans: Vec<LayerScan> = paths.iter().map(|_| LayerScan::new()).collect();
    for frame in 0..frame_count {
        let state = evaluate(graph, f64::from(frame));
        for visit in walk(graph, &state) {
            scans[visit.index].observe(frame, &visit, graph);
        }
    }

    let mut report = Report {
        violations: Vec::new(),
        passed: 0,
    };
    let mut rules_run = 0;
    for (ids, scan) in paths.iter().zip(scans) {
        let address = address_of(ids);
        // A fix hint edits the layer's `at` where the instance draws it.
        let layer = scene.layer(&address.layers);
        let placed = layer.zip(graph.node_at(ids));
        rules_run += scan.finish(&address, placed, graph, &mut report);
    }
    Span::current()
        .record("rules_run", rules_run)
        .record("violations", report.violations.len())
        .record("passed", report.passed);
    report
}

/// What the pass over the frames has seen of one layer.
struct LayerScan {
    /// The failing runs of each rule, parallel to [`RULES`].
    runs: Vec<Runs>,
    /// The box the layer covers over its visible frames. Fix hints clamp this whole
    /// sweep, since moving `at` moves every frame, so they settle in one edit.
    sweep: Option<Rect>,
    /// Where its `at` lives: the transform from its parent's coordinates to the
    /// canvas over its visible frames.
    frame: Frame,
}

/// The transform a layer's `at` is read through, over the frames seen so far.
#[derive(Clone, Copy)]
enum Frame {
    /// No visible frame yet.
    Unseen,
    /// The same at every visible frame.
    Steady(Affine),
    /// It changes, so no single `at` edit moves every frame by the same amount.
    Moving,
}

impl Frame {
    /// The frame after also seeing `parent`.
    fn and(self, parent: Affine) -> Frame {
        match self {
            Frame::Unseen => Frame::Steady(parent),
            Frame::Steady(seen) if same(seen, parent) => self,
            Frame::Steady(_) | Frame::Moving => Frame::Moving,
        }
    }
}

/// Whether two transforms agree up to float noise.
fn same(a: Affine, b: Affine) -> bool {
    a.as_coeffs()
        .iter()
        .zip(b.as_coeffs())
        .all(|(x, y)| (x - y).abs() < EPSILON)
}

impl LayerScan {
    fn new() -> LayerScan {
        LayerScan {
            runs: RULES.iter().map(|_| Runs::default()).collect(),
            sweep: None,
            frame: Frame::Unseen,
        }
    }

    /// Feeds the layer's state at `frame` to every rule and to the sweep.
    fn observe(&mut self, frame: u32, visit: &Visit<'_>, graph: &SceneGraph) {
        for (runs, rule) in self.runs.iter_mut().zip(RULES) {
            if let Some(found) = (rule.evaluate)(visit, graph) {
                runs.add(frame, found);
            }
        }
        if let Some(b) = visit.drawn().filter(|_| visible(visit)) {
            // Its parents' transform is its own world transform without its local one.
            let parent = visit.transform * local_transform(&visit.node.values).inverse();
            self.frame = self.frame.and(parent);
            self.sweep = Some(self.sweep.map_or(b, |a| a.union(b)));
        }
    }

    /// Adds the findings on the layer at `address` to `report` and returns
    /// how many rules ran; `placed` is the layer, its node and the
    /// composition graph it sits in, which a fix hint reads. A rule whose
    /// `suppressed_by` failed here is skipped: it neither runs nor passes.
    fn finish(
        self,
        address: &Address,
        placed: Option<(&Layer, (&Node, &CompGraph))>,
        graph: &SceneGraph,
        report: &mut Report,
    ) -> usize {
        let mut failed: Vec<Code> = Vec::new();
        let mut rules_run = 0;
        for (rule, Runs(runs)) in RULES.iter().zip(self.runs) {
            if rule.suppressed_by.is_some_and(|by| failed.contains(&by)) {
                continue;
            }
            rules_run += 1;
            if runs.is_empty() {
                report.passed += 1;
                continue;
            }
            failed.push(rule.code);
            let fix = self.sweep.zip(placed).and_then(|(sweep, placed)| {
                fix_hint(rule, sweep, self.frame, address, placed, graph)
            });
            let violations = runs
                .into_iter()
                .map(|run| run.into_violation(rule, address, fix.clone()));
            report.violations.extend(violations);
        }
        rules_run
    }
}

/// One rule's failing runs on one layer, in frame order; the last may still grow.
#[derive(Default)]
struct Runs(Vec<Run>);

impl Runs {
    /// Records a failure at `frame`, later than any recorded so far.
    fn add(&mut self, frame: u32, found: Overshoot) {
        match self.0.last_mut() {
            Some(run) if run.last + 1 == frame => run.extend(frame, found),
            _ => self.0.push(Run {
                first: frame,
                last: frame,
                worst: found,
            }),
        }
    }
}

/// A stretch of consecutive failing frames for one (rule, layer).
struct Run {
    first: u32,
    last: u32,
    /// The worst overshoot in the stretch.
    worst: Overshoot,
}

impl Run {
    fn extend(&mut self, frame: u32, found: Overshoot) {
        self.last = frame;
        if found.amount > self.worst.amount {
            self.worst = found;
        }
    }

    fn into_violation(self, rule: &Rule, address: &Address, fix: Option<String>) -> Violation {
        // Whole pixels, ignoring float noise: 55.000000000000007 reads +55, not +56.
        let amount = (self.worst.amount - EPSILON).ceil();
        Violation {
            severity: rule.severity,
            code: rule.code,
            address: address.clone(),
            first_frame: self.first,
            last_frame: self.last,
            amount_px: amount,
            fix,
        }
    }
}

/// The `at` edit that moves `sweep` into `rule`'s aim, on the axes that follow
/// `at`. The shift is on the canvas; `at` lives in the parent's coordinates, so
/// it goes back through the parents' transform, which must hold still.
fn fix_hint(
    rule: &Rule,
    sweep: Rect,
    frame: Frame,
    address: &Address,
    (layer, (node, comp)): (&Layer, (&Node, &CompGraph)),
    graph: &SceneGraph,
) -> Option<String> {
    let Frame::Steady(parent) = frame else {
        return None;
    };
    let (area, aim) = (rule.areas)(graph);
    let Vec2 { x: dx, y: dy } = shift(sweep, area, aim);
    let inverse = parent.inverse();
    let local = inverse * Point::new(dx, dy) - inverse * Point::ORIGIN;
    if !(local.x.is_finite() && local.y.is_finite()) {
        return None;
    }
    // Moving `at` cannot shift an axis that does not follow it; a turned
    // parent mixes the axes, so then both must follow.
    let (follows_x, follows_y) = node.follows_at;
    let mixes = parent.as_coeffs()[1].abs() > EPSILON || parent.as_coeffs()[2].abs() > EPSILON;
    if mixes && !(follows_x && follows_y) {
        return None;
    }
    let only_followed = |delta: f64, follows: bool| if follows { delta } else { 0.0 };
    // `at` keywords resolve in the parent's box, or on the canvas of the
    // composition the layer sits in.
    let extent = node.parent.map_or((comp.width, comp.height), |p| {
        let parent = comp.nodes[p].bounds;
        (parent.width(), parent.height())
    });
    fix(
        address,
        layer.at,
        Vec2::new(
            only_followed(snap(local.x), follows_x),
            only_followed(snap(local.y), follows_y),
        ),
        extent,
    )
}

/// A shift with float noise removed, so a whole-pixel shift prints whole.
fn snap(v: f64) -> f64 {
    let near = v.round();
    if (v - near).abs() < 1e-9 { near } else { v }
}

/// `id at="x y"` for a shift, or `None` if nothing moves. An axis that does not
/// move keeps its coordinate, so `center` stays `center`.
fn fix(
    address: &Address,
    at: Position,
    Vec2 { x: dx, y: dy }: Vec2,
    (width, height): (f64, f64),
) -> Option<String> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    let moved = |c: Coord, delta: f64, extent: f64| match delta {
        0.0 => c,
        d => Coord::Px(c.resolve(extent) + d),
    };
    let (x, y) = (moved(at.x, dx, width), moved(at.y, dy, height));
    Some(format!("{address} {AT}=\"{x} {y}\""))
}

#[cfg(test)]
#[path = "../tests/unit/check.rs"]
mod tests;
