//! The `check` query: every rule over every frame, with consecutive failing frames
//! merged into one [`Violation`] so the report stays short.
//!
//! A check is kept as a [`Checked`]: each layer's findings, and what the
//! scene's rules (`overlap`, `crowd`) need of it at each frame. The check
//! of an edited scene reads again only the layers the edit can change and
//! keeps the rest ([D79]); a test holds it to the check from scratch.
//!
//! [D79]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d79-an-edit-checks-only-what-it-can-change

use crate::report::{Code, Measure, Report, Severity, Violation};
use crate::rules::{
    CRAFT, Craft, EPSILON, Finding, Measured, Move, Overshoot, RULES, Reads, Rule, Sample, crowd,
    draws, overlap, shift, text_hold, visible,
};
use kurbo::{Affine, Point, Rect, Vec2};
use nakkash_format::address::SCENE;
use nakkash_format::key::AT;
use nakkash_format::shorthand::{Coord, Position};
use nakkash_format::{Address, Layer, Scene};
use nakkash_scene::pin::Fault;
use nakkash_scene::{
    CompGraph, ContentState, Node, SceneGraph, Visit, address_of, changed_paths, evaluate,
    local_transform, walk,
};
use std::collections::BTreeMap;
use std::num::NonZero;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use tracing::{Span, field, instrument};

/// Runs the standard rules on `scene` over every frame of `graph`, its
/// compiled form, on every node at any depth, in scene space, on as many
/// threads as the process works on ([`nakkash_machine::threads`],
/// [`check_on`]).
pub fn check(scene: &Scene, graph: &SceneGraph) -> Report {
    let threads = nakkash_machine::threads();
    check_on(
        scene,
        graph,
        NonZero::new(threads).unwrap_or(NonZero::<usize>::MIN),
    )
}

/// [`check`] on at most `threads` threads. The report is the same on any
/// number of threads, to the bit and in the same order: frames are
/// evaluated side by side, but every layer reads its frames in order.
pub fn check_on(scene: &Scene, graph: &SceneGraph, threads: NonZero<usize>) -> Report {
    Checked::new(scene, graph, threads).report().clone()
}

/// The check of one version of a scene, kept so that the check of the next
/// version, after an edit, reads again only what the edit can change
/// ([D79]).
///
/// [D79]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d79-an-edit-checks-only-what-it-can-change
#[derive(Debug, Clone)]
pub struct Checked {
    /// What each node's rules found, parallel to [`SceneGraph::paths`].
    layers: Vec<LayerFound>,
    /// The runs of frames each pair of texts, under then over by their
    /// place in the paths, run into each other at.
    overlaps: BTreeMap<(usize, usize), Vec<Run>>,
    /// The whole frames checked.
    frames: u32,
    /// The report the parts above make.
    report: Report,
}

/// What one node's own rules found over the frames, and what the scene's
/// rules need of it.
#[derive(Debug, Clone, Default)]
struct LayerFound {
    /// Its violations, in report order.
    violations: Vec<Violation>,
    /// How many of its rules ran and found nothing.
    passed: usize,
    /// How many of its rules ran.
    rules_run: usize,
    /// The frames at which it catches the eye, in order, for `crowd`.
    catches: Vec<u32>,
    /// The frames at which it is text being read, in order, with the boxes
    /// its glyphs draw there, for `overlap`.
    reads: Vec<(u32, Vec<Rect>)>,
}

impl Checked {
    /// Checks `scene`, compiled as `graph`, from scratch on at most
    /// `threads` threads.
    #[instrument(
        name = "check",
        level = "debug",
        skip_all,
        fields(
            frames = field::Empty,
            rules_run = field::Empty,
            violations = field::Empty,
            passed = field::Empty
        )
    )]
    pub fn new(scene: &Scene, graph: &SceneGraph, threads: NonZero<usize>) -> Checked {
        debug_assert_eq!(scene.root.layers.len(), graph.root.nodes.len());
        let paths = graph.paths();
        let every = vec![true; paths.len()];
        let checked = Checked::read(scene, graph, &paths, &every, None, threads.get());
        checked.record();
        checked
    }

    /// The check of `after`, compiled as `graph`: the scene this check was
    /// made of, `before` compiled as `was`, with an edit made. Only the
    /// layers the edit can change are read again ([`changed_layers`]); the
    /// report is the one [`Checked::new`] makes of `after`, to the bit.
    #[instrument(
        name = "check",
        level = "debug",
        skip_all,
        fields(
            frames = field::Empty,
            changed = field::Empty,
            rules_run = field::Empty,
            violations = field::Empty,
            passed = field::Empty
        )
    )]
    pub fn after(
        &self,
        (before, was): (&Scene, &SceneGraph),
        (after, graph): (&Scene, &SceneGraph),
        threads: NonZero<usize>,
    ) -> Checked {
        let paths = graph.paths();
        let changed = changed_layers((before, was), (after, graph), &paths)
            .filter(|_| frames_of(graph) == self.frames && self.layers.len() == paths.len());
        let checked = match changed {
            Some(changed) => {
                Span::current().record("changed", changed.iter().filter(|&&c| c).count());
                Checked::read(after, graph, &paths, &changed, Some(self), threads.get())
            }
            None => {
                let every = vec![true; paths.len()];
                Checked::read(after, graph, &paths, &every, None, threads.get())
            }
        };
        checked.record();
        checked
    }

    /// The report.
    pub fn report(&self) -> &Report {
        &self.report
    }

    /// The report, kept no longer.
    pub fn into_report(self) -> Report {
        self.report
    }

    fn record(&self) {
        Span::current()
            .record("frames", self.frames)
            .record("violations", self.report.violations.len())
            .record("passed", self.report.passed);
    }

    /// Reads the layers flagged in `fresh` over every frame, takes the
    /// others from `kept`, and judges the scene's rules over both.
    fn read(
        scene: &Scene,
        graph: &SceneGraph,
        paths: &[Vec<&str>],
        fresh: &[bool],
        kept: Option<&Checked>,
        threads: usize,
    ) -> Checked {
        let frames = frames_of(graph);
        let addresses: Vec<Address> = paths.iter().map(|ids| address_of(ids)).collect();
        // What the kept layers read at each frame, for text on text.
        let mut kept_reads: Vec<Vec<(usize, &[Rect])>> = vec![Vec::new(); frames as usize];
        if let Some(kept) = kept {
            for (index, layer) in kept.layers.iter().enumerate() {
                if fresh[index] {
                    continue;
                }
                for (frame, inks) in &layer.reads {
                    kept_reads[*frame as usize].push((index, inks));
                }
            }
        }
        let mut pass = Pass::new(graph, paths, fresh, threads);
        let mut overlaps: BTreeMap<(usize, usize), Vec<Run>> = match kept {
            Some(kept) => kept
                .overlaps
                .iter()
                .filter(|((under, over), _)| !fresh[*under] && !fresh[*over])
                .map(|(pair, runs)| (*pair, runs.clone()))
                .collect(),
            None => BTreeMap::new(),
        };
        pass.run(graph, 0..frames, &kept_reads, &mut overlaps);
        let mut layers = pass.finish(scene, graph, paths, &addresses);
        if let Some(kept) = kept {
            for (index, layer) in layers.iter_mut().enumerate() {
                if !fresh[index] {
                    *layer = kept.layers[index].clone();
                }
            }
        }
        let report = assemble(&layers, &overlaps, &addresses, frames);
        Checked {
            layers,
            overlaps,
            frames,
            report,
        }
    }
}

/// Which layers, parallel to `paths`, the edit from `before` to `after` can
/// change the check of: those a walk may meet differently
/// ([`changed_paths`]), and those whose layer in the scene changed, which
/// fix hints read. `None` when every layer may change.
pub fn changed_layers(
    (before, was): (&Scene, &SceneGraph),
    (after, graph): (&Scene, &SceneGraph),
    paths: &[Vec<&str>],
) -> Option<Vec<bool>> {
    let mut changed = changed_paths(was, graph)?;
    for (flag, ids) in changed.iter_mut().zip(paths) {
        if !*flag {
            let address = address_of(ids);
            *flag = before.layer(&address.layers) != after.layer(&address.layers);
        }
    }
    Some(changed)
}

/// The report the parts of a check make: each layer's findings in the
/// paths' order, then text on text, then the crowd.
fn assemble(
    layers: &[LayerFound],
    overlaps: &BTreeMap<(usize, usize), Vec<Run>>,
    addresses: &[Address],
    frames: u32,
) -> Report {
    let mut report = Report {
        violations: Vec::with_capacity(
            layers.iter().map(|l| l.violations.len()).sum::<usize>()
                + overlaps.values().map(Vec::len).sum::<usize>(),
        ),
        passed: 0,
    };
    let mut rules_run = 0;
    let mut movers = vec![0_usize; frames as usize];
    let mut read = vec![0_usize; frames as usize];
    for layer in layers {
        report.violations.extend(layer.violations.iter().cloned());
        report.passed += layer.passed;
        rules_run += layer.rules_run;
        for &frame in &layer.catches {
            movers[frame as usize] += 1;
        }
        for (frame, _) in &layer.reads {
            read[*frame as usize] += 1;
        }
    }
    // Text on text is the scene's to judge once two texts are read at once.
    if read.iter().any(|&n| n > 1) {
        rules_run += 1;
        if overlaps.is_empty() {
            report.passed += 1;
        }
    }
    for ((under, over), runs) in overlaps {
        report.violations.extend(runs.iter().map(|run| Violation {
            severity: Severity::Warning,
            code: Code::Overlap,
            address: addresses[*over].clone(),
            first_frame: run.first,
            last_frame: run.last,
            measure: Measure::Overshoot((run.worst.value - EPSILON).ceil()),
            fix: None,
            with: Some(addresses[*under].clone()),
        }));
    }
    // The crowd is the scene's: one rule over the whole clip, measured
    // once something moves.
    if movers.iter().any(|&n| n > 0) {
        rules_run += 1;
        let mut crowds: Vec<Run> = Vec::new();
        for (frame, &n) in (0_u32..).zip(&movers) {
            if let Some(found) = crowd::judge(n) {
                add(&mut crowds, frame, found);
            }
        }
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
                with: None,
            }));
    }
    Span::current().record("rules_run", rules_run);
    report
}

/// The whole frames `graph` lasts.
fn frames_of(graph: &SceneGraph) -> u32 {
    graph.root.duration_frames.ceil().max(0.0) as u32
}

/// What one frame shows the check of one node, read from its visit: all a
/// layer's rules need of the frame, so the frames can be read side by
/// side and then handed to each layer in order.
struct Look {
    /// The node's place in [`SceneGraph::paths`].
    index: usize,
    /// The box it draws on the canvas, when it is visible and draws.
    drawn: Option<Rect>,
    /// What each layout rule finds at the frame, parallel to [`RULES`].
    found: Vec<Option<Overshoot>>,
    /// Its parents' transform to the canvas, when it draws.
    parent: Option<Affine>,
    /// What craft rules see of it.
    sample: Sample,
    /// Where its glyphs draw on the canvas, for a text that may be read at
    /// the frame ([`text_hold::may_hold`]); whether it is read depends on
    /// the frame before.
    inks: Option<Vec<Rect>>,
    /// Whether it has a corner pin, and why the first that makes no quad
    /// at the frame does not, when it is visible then.
    pin: Option<Option<Fault>>,
}

/// What every node flagged in `fresh` shows at one frame; `laid` keeps the
/// glyph boxes texts are laid out in, read once per node.
fn look_at(
    graph: &SceneGraph,
    frame: u32,
    fresh: &[bool],
    blurred: &[bool],
    laid: &[OnceLock<Vec<Option<Rect>>>],
) -> Vec<Look> {
    let state = evaluate(graph, f64::from(frame));
    walk(graph, &state)
        .filter(|visit| fresh[visit.index])
        .map(|visit| {
            let drawn = visit.drawn().filter(|_| visible(&visit) && draws(&visit));
            let found = RULES
                .iter()
                .map(|rule| (rule.evaluate)(&visit, graph))
                .collect();
            // Its parents' transform is its own world transform without its local one.
            let parent =
                drawn.map(|_| visit.transform * local_transform(&visit.node.values).inverse());
            let sample = sample(frame, &visit, blurred[visit.index]);
            let inks = text_hold::may_hold(&sample).then(|| {
                let inks = laid[visit.index].get_or_init(|| overlap::layout_inks(&visit));
                let mut inks = overlap::canvas_inks(&visit, inks);
                overlap::sort_inks(&mut inks);
                inks
            });
            let pin = (!visit.node.pins.is_empty()).then(|| {
                visit
                    .node
                    .pins
                    .iter()
                    .find_map(|pin| pin.fault)
                    .filter(|_| visit.visible && visit.opacity > 0.0)
            });
            Look {
                index: visit.index,
                drawn,
                found,
                parent,
                sample,
                inks,
                pin,
            }
        })
        .collect()
}

/// What craft rules see of the layer at `frame`.
fn sample(frame: u32, visit: &Visit<'_>, blurred: bool) -> Sample {
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
        blurred,
    }
}

/// One pass of the rules over a scene's frames, for the layers flagged in
/// `fresh`: what each has seen so far.
struct Pass<'a> {
    /// The layers read, parallel to the paths.
    fresh: &'a [bool],
    /// What each node has seen, parallel to the paths.
    scans: Vec<LayerScan>,
    /// What the scene's rules need of each node, parallel to the paths.
    found: Vec<LayerFound>,
    /// Whether each node is drawn with motion blur where its composition's
    /// shutter is open, parallel to the paths.
    blurred: Vec<bool>,
    /// Where each text node's glyphs draw as laid out, read once.
    laid: Vec<OnceLock<Vec<Option<Rect>>>>,
    /// The most threads frames are read on.
    threads: usize,
}

/// How many frames each thread reads before the layers take them in
/// order: enough to keep threads busy, few enough that memory grows with
/// the layers, not the frames.
const FRAMES_A_THREAD: usize = 2;

impl<'a> Pass<'a> {
    fn new(graph: &SceneGraph, paths: &[Vec<&str>], fresh: &'a [bool], threads: usize) -> Pass<'a> {
        let blurred: Vec<bool> = paths
            .iter()
            .map(|ids| {
                // Motion blur smooths its steps where its composition's shutter is open.
                graph
                    .node_at(ids)
                    .is_some_and(|(node, comp)| node.mblur && comp.shutter.open())
            })
            .collect();
        Pass {
            fresh,
            scans: blurred.iter().map(|_| LayerScan::new()).collect(),
            found: blurred.iter().map(|_| LayerFound::default()).collect(),
            laid: blurred.iter().map(|_| OnceLock::new()).collect(),
            blurred,
            threads,
        }
    }

    /// Reads `frames`, in blocks read side by side and taken in order, and
    /// adds to `overlaps` the texts that run into a fresh one, with
    /// `kept_reads` the texts kept layers read at each frame.
    fn run(
        &mut self,
        graph: &SceneGraph,
        frames: Range<u32>,
        kept_reads: &[Vec<(usize, &[Rect])>],
        overlaps: &mut BTreeMap<(usize, usize), Vec<Run>>,
    ) {
        let block = (self.threads * FRAMES_A_THREAD).max(1) as u32;
        let mut start = frames.start;
        while start < frames.end {
            let end = frames.end.min(start.saturating_add(block));
            let (fresh, blurred, laid) = (self.fresh, &self.blurred, &self.laid);
            let looks = in_parallel(start..end, self.threads, |frame| {
                look_at(graph, frame, fresh, blurred, laid)
            });
            let reads: Vec<Vec<(usize, usize)>> = (start..end)
                .zip(looks)
                .map(|(frame, looks)| self.take(frame, looks, graph))
                .collect();
            let found = in_parallel(start..end, self.threads, |frame| {
                let at = (frame - start) as usize;
                let mut read: Vec<(usize, &[Rect])> = reads[at]
                    .iter()
                    .map(|&(index, at)| (index, self.found[index].reads[at].1.as_slice()))
                    .chain(kept_reads[frame as usize].iter().copied())
                    .collect();
                read.sort_unstable_by_key(|(index, _)| *index);
                overlapping(&read, fresh, graph)
            });
            for (frame, pairs) in (start..end).zip(found) {
                for (pair, finding) in pairs {
                    add(overlaps.entry(pair).or_default(), frame, finding);
                }
            }
            start = end;
        }
    }

    /// Hands every look at `frame` to its node's scan, in order; returns
    /// the nodes read as text at the frame, each with the place of the
    /// read in its [`LayerFound::reads`].
    fn take(&mut self, frame: u32, looks: Vec<Look>, graph: &SceneGraph) -> Vec<(usize, usize)> {
        let mut read = Vec::new();
        for look in looks {
            let index = look.index;
            let (catches, inks) = self.scans[index].observe(frame, look, graph);
            let found = &mut self.found[index];
            if catches {
                found.catches.push(frame);
            }
            if let Some(inks) = inks {
                read.push((index, found.reads.len()));
                found.reads.push((frame, inks));
            }
        }
        read
    }

    /// What each fresh layer found, once the frames have run out.
    fn finish(
        self,
        scene: &Scene,
        graph: &SceneGraph,
        paths: &[Vec<&str>],
        addresses: &[Address],
    ) -> Vec<LayerFound> {
        let fresh = self.fresh;
        paths
            .iter()
            .zip(addresses)
            .zip(self.scans.into_iter().zip(self.found))
            .enumerate()
            .map(|(index, ((ids, address), (scan, mut found)))| {
                if fresh[index] {
                    // A fix hint edits the layer's `at` where the instance draws it.
                    let layer = scene.layer(&address.layers);
                    let placed = layer.zip(graph.node_at(ids));
                    let mut report = Report {
                        violations: Vec::new(),
                        passed: 0,
                    };
                    found.rules_run = scan.finish(address, placed, graph, &mut report);
                    found.violations = report.violations;
                    found.passed = report.passed;
                }
                found
            })
            .collect()
    }
}

/// Every pair of the texts `read` at one frame, in the paths' order, whose
/// glyphs run into each other and of which one at least is `fresh`: under
/// then over, by their place in the paths, with how deep.
///
/// Pairs whose glyphs' reaches cannot meet are passed over: the texts are
/// swept from left to right by where their reach starts, and a text meets
/// only those that start before its reach ends.
fn overlapping(
    read: &[(usize, &[Rect])],
    fresh: &[bool],
    graph: &SceneGraph,
) -> Vec<((usize, usize), Finding)> {
    let mut reaching: Vec<(usize, Rect)> = read
        .iter()
        .enumerate()
        .filter_map(|(k, (_, inks))| overlap::reach(inks).map(|r| (k, r)))
        .collect();
    reaching.sort_by(|a, b| a.1.x0.total_cmp(&b.1.x0).then(a.0.cmp(&b.0)));
    let mut found = Vec::new();
    for (i, &(a, ra)) in reaching.iter().enumerate() {
        for &(b, rb) in &reaching[i + 1..] {
            // Sorted by where they start, none further on meets this one.
            if rb.x0 > ra.x1 {
                break;
            }
            let (under, over) = (read[a.min(b)], read[a.max(b)]);
            if !(fresh[under.0] || fresh[over.0]) || !ra.overlaps(rb) {
                continue;
            }
            if let Some(finding) = overlap::judge(under.1, over.1, graph) {
                found.push(((under.0, over.0), finding));
            }
        }
    }
    found
}

/// `work` on every item of `items`, on up to `threads` threads, each
/// taking the next item left; the results in the items' order.
fn in_parallel<T: Send>(
    items: Range<u32>,
    threads: usize,
    work: impl Fn(u32) -> T + Sync,
) -> Vec<T> {
    let count = items.len();
    let threads = threads.min(count).max(1);
    if threads == 1 {
        return items.map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, T)>> = Mutex::new(Vec::with_capacity(count));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= count {
                        break;
                    }
                    let result = work(items.start + i as u32);
                    done.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((i, result));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(PoisonError::into_inner);
    done.sort_unstable_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, result)| result).collect()
}

/// What the pass over the frames has seen of one layer.
struct LayerScan {
    /// What each layout rule has seen, parallel to [`RULES`].
    runs: Vec<LayoutScan>,
    /// The box the layer covers over its visible frames on the canvas. Fix hints
    /// clamp this whole sweep, since moving `at` moves every frame, so they settle
    /// in one edit.
    sweep: Option<Rect>,
    /// The box it covers over its frames wholly off the canvas, which fix
    /// hints clamp instead when it never comes on.
    off_sweep: Option<Rect>,
    /// The frame it was last seen at and the box it drew there, to tell a
    /// layer in motion from one at rest.
    seen: Option<(u32, Rect)>,
    /// Where its `at` lives: the transform from its parent's coordinates to the
    /// canvas over its visible frames.
    frame: Frame,
    /// What craft rules saw of it at the frame before, if it was walked.
    previous: Option<Sample>,
    /// Each craft rule's reading of it, parallel to [`CRAFT`].
    craft: Vec<CraftScan>,
    /// What the `pin` rule has seen of it.
    pin: PinScan,
}

/// What the `pin` rule has seen of one layer: whether it has a corner pin,
/// and the runs of frames at which one makes no quad, each with why at its
/// first frame.
#[derive(Default)]
struct PinScan {
    /// It has a corner pin at some frame, so the rule runs on it.
    ran: bool,
    /// The failing runs: first and last frame, and the fault.
    runs: Vec<(u32, u32, Fault)>,
}

impl PinScan {
    /// Reads the layer at `frame`, later than any seen so far.
    fn observe(&mut self, frame: u32, pin: Option<Option<Fault>>) {
        let Some(fault) = pin else {
            return;
        };
        self.ran = true;
        let Some(fault) = fault else {
            return;
        };
        match self.runs.last_mut() {
            Some((_, last, _)) if *last + 1 == frame => *last = frame,
            _ => self.runs.push((frame, frame, fault)),
        }
    }
}

/// What one craft rule has seen of one layer.
#[derive(Default)]
struct CraftScan {
    /// Its failing runs.
    runs: Vec<Run>,
    /// The stretch now open, for a stretch rule: its first sample and its
    /// last frame.
    open: Option<(Sample, u32)>,
    /// The longest stretch closed so far, the first of the longest on a tie.
    longest: Option<(Sample, u32)>,
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
            Reads::Stretch { holds, .. } => {
                if before.is_none() {
                    self.close();
                }
                if !holds(now, before, graph) {
                    self.close();
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
            Reads::Stretch { judge, .. } => {
                self.close();
                self.judge_longest(judge, graph);
            }
            Reads::Moves { judge, .. } => self.stop(judge, graph),
        }
    }

    /// Closes the open stretch, if any, keeping it when it is the longest.
    fn close(&mut self) {
        let Some((first, last)) = self.open.take() else {
            return;
        };
        let frames = |first: &Sample, last: u32| last - first.frame + 1;
        let longer = self
            .longest
            .as_ref()
            .is_none_or(|(f, l)| frames(&first, last) > frames(f, *l));
        if longer {
            self.longest = Some((first, last));
        }
    }

    /// Judges the longest stretch, once the frames have run out.
    fn judge_longest(
        &mut self,
        judge: fn(u32, &Sample, &SceneGraph) -> Option<Finding>,
        graph: &SceneGraph,
    ) {
        let Some((first, last)) = self.longest.take() else {
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
    fn new() -> LayerScan {
        LayerScan {
            runs: RULES.iter().map(|_| LayoutScan::default()).collect(),
            sweep: None,
            off_sweep: None,
            seen: None,
            frame: Frame::Unseen,
            previous: None,
            craft: CRAFT.iter().map(|_| CraftScan::default()).collect(),
            pin: PinScan::default(),
        }
    }

    /// Feeds the layer's look at `frame` to every rule and to the sweep, and
    /// says whether it catches the eye over the step to it and, when it is
    /// text being read, where its glyphs draw.
    fn observe(&mut self, frame: u32, look: Look, graph: &SceneGraph) -> (bool, Option<Vec<Rect>>) {
        self.layout(frame, &look, graph);
        self.pin.observe(frame, look.pin);
        let now = look.sample;
        let before = self.previous.take().filter(|b| b.frame + 1 == frame);
        for (scan, rule) in self.craft.iter_mut().zip(CRAFT) {
            scan.observe(rule, &now, before.as_ref(), graph);
        }
        let catches = before
            .as_ref()
            .is_some_and(|b| crowd::catches_the_eye(b, &now, graph));
        let readable = text_hold::holds(&now, before.as_ref(), graph);
        self.previous = Some(now);
        (catches, look.inks.filter(|_| readable))
    }

    /// Feeds the layer's look at `frame` to the layout rules and the sweep.
    fn layout(&mut self, frame: u32, look: &Look, graph: &SceneGraph) {
        let drawn = look.drawn;
        let (width, height) = (graph.root.width, graph.root.height);
        // Off the canvas: on or past one of its edges, so a box of no size
        // inside it is on it.
        let off = |b: Rect| b.x1 <= 0.0 || b.y1 <= 0.0 || b.x0 >= width || b.y0 >= height;
        let before = self.seen.filter(|&(at, _)| at + 1 == frame).map(|(_, b)| b);
        let place = match drawn {
            None => Place::Unseen,
            Some(b) if off(b) => Place::Off,
            Some(b) if before.is_some_and(|a| same_box(a, b)) => Place::Resting,
            Some(_) => Place::Moving,
        };
        self.seen = drawn.map(|b| (frame, b));
        let brief = brief(graph);
        for (scan, found) in self.runs.iter_mut().zip(&look.found) {
            scan.observe(frame, place, *found, brief);
        }
        if let (Some(b), Some(parent)) = (drawn, look.parent) {
            self.frame = self.frame.and(parent);
            let sweep = match place {
                Place::Off => &mut self.off_sweep,
                _ => &mut self.sweep,
            };
            *sweep = Some(sweep.map_or(b, |a| a.union(b)));
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
        for (rule, scan) in RULES.iter().zip(self.runs) {
            let runs = scan.finish(brief(graph));
            if rule.suppressed_by.is_some_and(|by| failed.contains(&by)) {
                continue;
            }
            rules_run += 1;
            if runs.is_empty() {
                report.passed += 1;
                continue;
            }
            failed.push(rule.code);
            let fix = self
                .sweep
                .or(self.off_sweep)
                .zip(placed)
                .and_then(|(sweep, placed)| {
                    fix_hint(rule, sweep, self.frame, address, placed, graph)
                });
            let violations = runs
                .into_iter()
                .map(|run| run.into_violation(rule, address, fix.clone()));
            report.violations.extend(violations);
        }
        // A corner pin that makes no quad leaves its layer undrawn ([D89]).
        //
        // [D89]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography
        if self.pin.ran {
            rules_run += 1;
            match self.pin.runs.is_empty() {
                true => report.passed += 1,
                false => report.violations.extend(self.pin.runs.into_iter().map(
                    |(first, last, fault)| Violation {
                        severity: Severity::Error,
                        code: Code::Pin,
                        address: address.clone(),
                        first_frame: first,
                        last_frame: last,
                        measure: Measure::Pin(fault),
                        fix: None,
                        with: None,
                    },
                )),
            }
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
                    with: None,
                }));
        }
        rules_run
    }
}

/// Where a layer is at one frame, for the layout rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Place {
    /// It draws nothing: hidden, see-through, out of its span or clipped away.
    Unseen,
    /// It draws wholly outside the canvas, where it waits to come in or
    /// has gone.
    Off,
    /// It draws on the canvas the same box as at the frame before.
    Resting,
    /// It draws on the canvas, and moved or first showed at this frame.
    Moving,
}

/// Whether two boxes agree up to float noise.
fn same_box(a: Rect, b: Rect) -> bool {
    [(a.x0, b.x0), (a.y0, b.y0), (a.x1, b.x1), (a.y1, b.y1)]
        .iter()
        .all(|(x, y)| (x - y).abs() < 1e-6)
}

/// The longest a layer may move across the edge and not be checked there,
/// in seconds: the least time any text is held to be read (`text_hold`),
/// so nothing that crosses for less can be read cut off.
const BRIEF: f64 = 0.5;

/// [`BRIEF`] in whole frames of `graph`'s root, to the nearest: 15 at
/// 29.97 frames a second as at 30.
fn brief(graph: &SceneGraph) -> usize {
    (BRIEF * graph.root.fps).round() as usize
}

/// What one layout rule has seen of one layer, in frame order.
///
/// A layer waiting wholly off the canvas is not checked: nothing of it shows.
/// Nor is one passing through the edge on its way in from there or out to
/// there, as a push or slide does, nor one that moves across the edge for
/// no longer than [`BRIEF`] and then leaves it, inside or out of sight, as a
/// zoom through the frame or a glitch's copies do. These depend only on
/// where the layer moves and for how long, never on its size, so a master
/// and a smaller or larger copy of it are judged alike as they move. A layer at rest
/// across the edge is checked, as is one that moves across it for longer,
/// and one that never comes onto the canvas at all.
#[derive(Default)]
struct LayoutScan {
    /// The failing runs found so far; the last may still grow.
    runs: Vec<Run>,
    /// The failing frames of the move now under way, kept until the move ends
    /// and says whether it came from off the canvas or went there.
    passing: Vec<(u32, Finding)>,
    /// The move now under way came in from off the canvas.
    from_off: bool,
    /// The runs of frames it spent wholly off the canvas, with how far.
    off: Vec<Run>,
    /// It was on the canvas at some frame.
    on: bool,
    /// The last frame it was observed at.
    last: Option<u32>,
}

impl LayoutScan {
    /// Reads the layer at `frame`, later than any seen so far: where it is,
    /// and what the rule found there.
    fn observe(&mut self, frame: u32, place: Place, found: Option<Overshoot>, brief: usize) {
        if self.last.is_some_and(|last| last + 1 != frame) {
            self.settle(brief);
        }
        self.last = Some(frame);
        let found = found.map(|found| Finding {
            value: found.amount,
            limit: 0.0,
        });
        match (place, found) {
            (Place::Unseen, _) => self.settle(brief),
            (Place::Off, found) => {
                // The move reached the outside: it was a way in or out.
                self.passing.clear();
                self.from_off = true;
                if let Some(found) = found {
                    add(&mut self.off, frame, found);
                }
            }
            (Place::Moving, Some(found)) => {
                self.on = true;
                self.passing.push((frame, found));
            }
            (Place::Resting, Some(found)) => {
                // The move came to rest across the edge: all of it counts.
                self.on = true;
                self.settle(0);
                add(&mut self.runs, frame, found);
            }
            (Place::Resting | Place::Moving, None) => {
                self.on = true;
                self.settle(brief);
            }
        }
    }

    /// Ends the move under way: its failing frames count unless it came in
    /// from off the canvas, or lasted no more than `brief` frames.
    fn settle(&mut self, brief: usize) {
        let passed = self.from_off || self.passing.len() <= brief;
        for (frame, found) in self.passing.drain(..) {
            if !passed {
                add(&mut self.runs, frame, found);
            }
        }
        self.from_off = false;
    }

    /// The failing runs, once the frames run out: those it spent off the
    /// canvas count too when it never came onto it.
    fn finish(mut self, brief: usize) -> Vec<Run> {
        self.settle(brief);
        match self.on {
            true => self.runs,
            false => self.off,
        }
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
#[derive(Debug, Clone)]
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
            with: None,
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
