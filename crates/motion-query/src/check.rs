//! The `check` query: every rule over every frame, with consecutive failing frames
//! merged into one [`Violation`] so the report stays short.

use crate::report::{Code, Measure, Report, Severity, Violation};
use crate::rules::{
    CRAFT, Craft, EPSILON, Finding, Measured, Move, Overshoot, RULES, Reads, Rule, Sample, crowd,
    shift, visible,
};
use kurbo::{Affine, Point, Rect, Vec2};
use motion_format::address::SCENE;
use motion_format::key::AT;
use motion_format::shorthand::{Coord, Position};
use motion_format::{Address, Layer, Scene};
use motion_scene::{
    CompGraph, ContentState, Node, SceneGraph, Visit, address_of, evaluate, local_transform, walk,
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
    let mut scans: Vec<LayerScan> = paths
        .iter()
        .map(|ids| {
            // Motion blur smooths its steps where its composition's shutter is open.
            let blurred = graph
                .node_at(ids)
                .is_some_and(|(node, comp)| node.mblur && comp.shutter.open());
            LayerScan::new(blurred)
        })
        .collect();
    // How many layers catch the eye at each frame, for `crowd`.
    let mut crowds: Vec<Run> = Vec::new();
    let mut anything_moved = false;
    for frame in 0..frame_count {
        let state = evaluate(graph, f64::from(frame));
        let mut movers = 0;
        for visit in walk(graph, &state) {
            movers += usize::from(scans[visit.index].observe(frame, &visit, graph));
        }
        anything_moved |= movers > 0;
        if let Some(found) = crowd::judge(movers) {
            add(&mut crowds, frame, found);
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
    // The crowd is the scene's: one rule over the whole clip, measured
    // once something moves.
    if anything_moved {
        rules_run += 1;
        if crowds.is_empty() {
            report.passed += 1;
        }
        let scene: Address = SCENE.parse().expect("the scene's address");
        report
            .violations
            .extend(crowds.into_iter().map(|run| Violation {
                severity: Severity::Warning,
                code: Code::Crowd,
                address: scene.clone(),
                first_frame: run.first,
                last_frame: run.last,
                measure: crowd::measure(run.worst),
                fix: None,
            }));
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
    /// What craft rules saw of it at the frame before, if it was walked.
    previous: Option<Sample>,
    /// Whether it is drawn with motion blur.
    blurred: bool,
    /// Each craft rule's reading of it, parallel to [`CRAFT`].
    craft: Vec<CraftScan>,
}

/// What one craft rule has seen of one layer.
#[derive(Default)]
struct CraftScan {
    /// Its failing runs.
    runs: Vec<Run>,
    /// The stretch now open, for a stretch rule: its first sample and its
    /// last frame.
    open: Option<(Sample, u32)>,
    /// The move now open, for a move rule.
    moving: Option<Move>,
    /// Whether it measured anything: a step between two shown frames, or a
    /// whole stretch.
    ran: bool,
}

impl CraftScan {
    /// Reads `now`, given the sample of the frame before when there is one.
    fn observe(&mut self, rule: &Craft, now: &Sample, before: Option<&Sample>, graph: &SceneGraph) {
        match rule.reads {
            Reads::Step(step) => {
                let Some(before) = before.filter(|b| b.shown && now.shown) else {
                    return;
                };
                match step(before, now, graph) {
                    Measured::Skipped => {}
                    Measured::Passed => self.ran = true,
                    Measured::Found(found) => {
                        self.ran = true;
                        add(&mut self.runs, now.frame, found);
                    }
                }
            }
            Reads::Stretch { holds, judge } => {
                if before.is_none() {
                    self.close(judge, graph);
                }
                if !holds(now, before, graph) {
                    self.close(judge, graph);
                    return;
                }
                match &mut self.open {
                    Some((_, last)) => *last = now.frame,
                    None => self.open = Some((now.clone(), now.frame)),
                }
            }
            Reads::Moves { speed, judge } => {
                let Some(fast) = before.and_then(|b| speed(b, now)) else {
                    self.stop(judge, graph);
                    return;
                };
                match &mut self.moving {
                    Some(going) => {
                        going.last = now.frame;
                        going.total += fast;
                        going.peak = going.peak.max(fast);
                    }
                    None => {
                        self.moving = Some(Move {
                            first: now.frame - 1,
                            last: now.frame,
                            total: fast,
                            peak: fast,
                        });
                    }
                }
            }
        }
    }

    /// Judges the open move, if any, and ends it.
    fn stop(&mut self, judge: fn(&Move, &SceneGraph) -> Measured, graph: &SceneGraph) {
        let Some(going) = self.moving.take() else {
            return;
        };
        match judge(&going, graph) {
            Measured::Skipped => {}
            Measured::Passed => self.ran = true,
            Measured::Found(found) => {
                self.ran = true;
                self.runs.push(Run {
                    first: going.first,
                    last: going.last,
                    worst: found,
                });
            }
        }
    }

    /// Ends whatever is still open once the frames run out.
    fn end(&mut self, rule: &Craft, graph: &SceneGraph) {
        match rule.reads {
            Reads::Step(_) => {}
            Reads::Stretch { judge, .. } => self.close(judge, graph),
            Reads::Moves { judge, .. } => self.stop(judge, graph),
        }
    }

    /// Judges the open stretch, if any, and closes it.
    fn close(
        &mut self,
        judge: fn(u32, &Sample, &SceneGraph) -> Option<Finding>,
        graph: &SceneGraph,
    ) {
        let Some((first, last)) = self.open.take() else {
            return;
        };
        self.ran = true;
        if let Some(found) = judge(last - first.frame + 1, &first, graph) {
            self.runs.push(Run {
                first: first.frame,
                last,
                worst: found,
            });
        }
    }
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
    fn new(blurred: bool) -> LayerScan {
        LayerScan {
            runs: RULES.iter().map(|_| Runs::default()).collect(),
            sweep: None,
            frame: Frame::Unseen,
            previous: None,
            blurred,
            craft: CRAFT.iter().map(|_| CraftScan::default()).collect(),
        }
    }

    /// Feeds the layer's state at `frame` to every rule and to the sweep, and
    /// says whether it catches the eye over the step to it.
    fn observe(&mut self, frame: u32, visit: &Visit<'_>, graph: &SceneGraph) -> bool {
        self.layout(frame, visit, graph);
        let now = self.sample(frame, visit);
        let before = self.previous.take().filter(|b| b.frame + 1 == frame);
        for (scan, rule) in self.craft.iter_mut().zip(CRAFT) {
            scan.observe(rule, &now, before.as_ref(), graph);
        }
        let catches = before
            .as_ref()
            .is_some_and(|b| crowd::catches_the_eye(b, &now, graph));
        self.previous = Some(now);
        catches
    }

    /// Feeds the layer's state at `frame` to the layout rules and the sweep.
    fn layout(&mut self, frame: u32, visit: &Visit<'_>, graph: &SceneGraph) {
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

    /// What craft rules see of the layer at `frame`.
    fn sample(&self, frame: u32, visit: &Visit<'_>) -> Sample {
        let b = visit.node.own_box;
        let corners = [(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)]
            .map(|(x, y)| visit.transform * Point::new(x, y));
        let chars = match &visit.node.content {
            ContentState::Text { text, .. } => Some(
                text.content
                    .text
                    .chars()
                    .filter(|c| !c.is_control())
                    .count(),
            ),
            _ => None,
        };
        Sample {
            frame,
            shown: visible(visit),
            opacity: visit.opacity,
            corners,
            local: local_transform(&visit.node.values),
            chars,
            blurred: self.blurred,
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
        for (rule, mut scan) in CRAFT.iter().zip(self.craft) {
            scan.end(rule, graph);
            if !scan.ran {
                continue;
            }
            let lasting = (rule.lasting * graph.root.fps - EPSILON).ceil() as u32;
            scan.runs.retain(|run| run.last - run.first + 1 >= lasting);
            rules_run += 1;
            if scan.runs.is_empty() {
                report.passed += 1;
                continue;
            }
            let fix = placed
                .and_then(|(layer, (node, comp))| (rule.fix)(address, Some(layer), node, comp));
            report
                .violations
                .extend(scan.runs.into_iter().map(|run| Violation {
                    severity: rule.severity,
                    code: rule.code,
                    address: address.clone(),
                    first_frame: run.first,
                    last_frame: run.last,
                    measure: (rule.measure)(run.worst),
                    fix: fix.clone(),
                }));
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
        let found = Finding {
            value: found.amount,
            limit: 0.0,
        };
        add(&mut self.0, frame, found);
    }
}

/// Records `found` at `frame`, later than any frame in `runs`: it extends the
/// last run when that ends at the frame before.
fn add(runs: &mut Vec<Run>, frame: u32, found: Finding) {
    match runs.last_mut() {
        Some(run) if run.last + 1 == frame => run.extend(frame, found),
        _ => runs.push(Run {
            first: frame,
            last: frame,
            worst: found,
        }),
    }
}

/// A stretch of consecutive failing frames for one (rule, layer).
struct Run {
    first: u32,
    last: u32,
    /// The worst finding in the stretch.
    worst: Finding,
}

impl Run {
    fn extend(&mut self, frame: u32, found: Finding) {
        self.last = frame;
        if found.excess() > self.worst.excess() {
            self.worst = found;
        }
    }

    fn into_violation(self, rule: &Rule, address: &Address, fix: Option<String>) -> Violation {
        // Whole pixels, ignoring float noise: 55.000000000000007 reads +55, not +56.
        let amount = (self.worst.value - EPSILON).ceil();
        Violation {
            severity: rule.severity,
            code: rule.code,
            address: address.clone(),
            first_frame: self.first,
            last_frame: self.last,
            measure: Measure::Overshoot(amount),
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
