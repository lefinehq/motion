//! Expressions evaluated at import ([D38]): every property whose expression
//! is switched on is evaluated at each frame its layer shows, and what it
//! gave is kept as samples for the map to fit into keys.
//!
//! - **Order.** An expression may read another property with an expression.
//!   The read waits ([`motion_expr::Missing::Pending`]); the bake evaluates
//!   that one at the time asked, remembers it, and runs the first again. A
//!   cycle, or a chain deeper than [`MAX_DEPTH`], is reported.
//! - **Failure.** An expression that throws, reads what the import cannot
//!   know, or reads one that failed, is not baked, with why; the map keeps
//!   the keys under it. The original likewise stops using an expression
//!   that fails.
//! - **Frames.** A property is evaluated at the frames its layer shows,
//!   within the span of its comp the scene shows: a precomp an hour long
//!   that the scene shows for 30 seconds is evaluated for those 30.
//! - **Threads.** The engine recurses, so the bake runs on a thread with
//!   [`STACK_SIZE`] of stack.
//!
//! [D38]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d38-expressions-are-baked-at-import

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use motion_expr::{Dialect, Engine, Failure, Limits, NodeId, Program, STACK_SIZE, Val};

use crate::model::{Composition, ExpressionEngine, ItemKind, Project, Property, Stream, Value};

mod host;

use host::{Baked, ProjectHost, Target};

/// How deep expressions may wait on one another.
pub const MAX_DEPTH: usize = 64;

/// The most frames one expression is evaluated at.
const MAX_FRAMES: usize = 100_000;

/// How often one run may wait on another expression before it is stopped.
const MAX_WAITS: usize = 10_000;

/// Every expression of a project, evaluated.
#[derive(Debug, Default)]
pub struct Bakes {
    /// Each property's bake, by the property's address in the project.
    by_property: HashMap<usize, Bake>,
    /// Why nothing was baked, when the engine could not run at all.
    pub stopped: Option<String>,
}

impl Bakes {
    /// The bake of `property`, if it has an expression that is switched on
    /// and the engine ran.
    pub fn get(&self, property: &Property) -> Option<&Bake> {
        self.by_property.get(&address(property))
    }

    /// How many properties were evaluated.
    pub fn len(&self) -> usize {
        self.by_property.len()
    }

    /// Whether no property was evaluated.
    pub fn is_empty(&self) -> bool {
        self.by_property.is_empty()
    }
}

/// One property's expression, evaluated.
#[derive(Debug, Clone, PartialEq)]
pub struct Bake {
    /// The expression's first line.
    pub source: String,
    /// The engine it was evaluated as.
    pub dialect: Dialect,
    /// The legacy forms repaired before it compiled.
    pub repairs: Vec<String>,
    /// Its samples, or why it could not be evaluated.
    pub outcome: Result<Samples, String>,
}

/// What an expression gave at each frame of its layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Samples {
    /// The times, in composition seconds.
    pub times: Vec<f64>,
    /// The values at those times.
    pub values: Sampled,
    /// Whether it used noise or randomness, which is our own approximation.
    pub approximate: bool,
}

/// The values of [`Samples`].
#[derive(Debug, Clone, PartialEq)]
pub enum Sampled {
    /// Numbers, one channel per component of the property's value, in the
    /// project's units, each with the tolerance a fit of it may keep to.
    Numbers {
        /// One list of values per component.
        channels: Vec<Vec<f64>>,
        /// Per component, how far a fitted curve may stray from a sample.
        tolerances: Vec<f64>,
    },
    /// Text, as a source text expression gives it.
    Text(Vec<String>),
}

/// The key of `property` in [`Bakes`].
fn address(property: &Property) -> usize {
    std::ptr::from_ref(property).addr()
}

/// The dialect a project's engine evaluates.
pub fn dialect(engine: &ExpressionEngine) -> Dialect {
    match engine {
        ExpressionEngine::JavaScript => Dialect::Modern,
        ExpressionEngine::ExtendScript | ExpressionEngine::Other(_) => Dialect::Legacy,
    }
}

/// Evaluates every expression of `project` that is switched on, in the
/// comps the composition `root` (an item id) shows, or in every comp over
/// its whole duration without one.
pub fn bake(project: &Project, root: Option<u32>) -> Bakes {
    let spawned = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("motion-bake".into())
            .stack_size(STACK_SIZE)
            .spawn_scoped(scope, || bake_here(project, root))
            .map(|handle| handle.join())
    });
    match spawned {
        Ok(Ok(bakes)) => bakes,
        Ok(Err(_)) => Bakes {
            stopped: Some("the expression engine stopped with an internal error".into()),
            ..Bakes::default()
        },
        Err(error) => Bakes {
            stopped: Some(format!("no thread for the expression engine: {error}")),
            ..Bakes::default()
        },
    }
}

/// [`bake`], on the current thread.
fn bake_here(project: &Project, root: Option<u32>) -> Bakes {
    let (host, targets) = ProjectHost::new(project);
    let spans = spans(project, root);
    let targets: Vec<Target<'_>> = targets
        .into_iter()
        .filter(|t| spans.contains_key(&t.item))
        .collect();
    if targets.is_empty() {
        return Bakes::default();
    }
    let host = Rc::new(host);
    let dialect = dialect(&project.expression_engine);
    let engine = match Engine::new(host.clone(), dialect, Limits::default()) {
        Ok(engine) => engine,
        Err(error) => {
            return Bakes {
                stopped: Some(error.to_string()),
                ..Bakes::default()
            };
        }
    };
    let mut baker = Baker {
        host,
        engine,
        dialect,
        sources: targets
            .iter()
            .map(|t| {
                (
                    t.node,
                    t.property
                        .expression
                        .as_ref()
                        .map(|e| e.text.clone())
                        .unwrap_or_default(),
                )
            })
            .collect(),
        programs: HashMap::new(),
        repairs: HashMap::new(),
        active: HashSet::new(),
        approximate: HashSet::new(),
        spans,
    };
    let by_property = targets
        .iter()
        .map(|target| (address(target.property), baker.target(target)))
        .collect();
    Bakes {
        by_property,
        stopped: None,
    }
}

/// The bake's state over one project.
struct Baker {
    host: Rc<ProjectHost>,
    engine: Engine,
    dialect: Dialect,
    /// Each expression's source, by node.
    sources: HashMap<NodeId, String>,
    /// Each expression compiled, or why it did not compile.
    programs: HashMap<NodeId, Result<Program, String>>,
    /// The legacy forms repaired in each, by node.
    repairs: HashMap<NodeId, Vec<String>>,
    /// The evaluations under way, by node and time, to find cycles.
    active: HashSet<(NodeId, u64)>,
    /// The expressions that used noise or randomness.
    approximate: HashSet<NodeId>,
    /// The span of each comp the scene shows, by item id.
    spans: HashMap<u32, (f64, f64)>,
}

impl Baker {
    /// One property's bake, over the frames its layer shows.
    fn target(&mut self, target: &Target<'_>) -> Bake {
        let source = self.sources.get(&target.node).cloned().unwrap_or_default();
        let outcome = self.samples(target);
        Bake {
            source: source
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or_default()
                .trim()
                .to_owned(),
            dialect: self.dialect,
            repairs: self.repairs.get(&target.node).cloned().unwrap_or_default(),
            outcome,
        }
    }

    fn samples(&mut self, target: &Target<'_>) -> Result<Samples, String> {
        if target.layer.stretch < 0.0 {
            return Err("its layer runs backwards, which the bake does not follow yet".into());
        }
        let span = self
            .spans
            .get(&target.item)
            .copied()
            .unwrap_or((0.0, target.comp.duration));
        let times = frames(target, span)?;
        let mut values = Vec::with_capacity(times.len());
        for &time in &times {
            values.push(self.evaluate(target.node, time, 0)?);
        }
        let template = target.property.value.value_at(times[0]);
        Ok(Samples {
            values: sampled(target.property, &template, &times, &values)?,
            times,
            approximate: self.approximate.contains(&target.node),
        })
    }

    /// The value of the expression on `node` at `time`, evaluating the
    /// expressions it waits on first.
    fn evaluate(&mut self, node: NodeId, time: f64, depth: usize) -> Result<Val, String> {
        let bits = time.to_bits();
        match self.host.baked.borrow().get(&node) {
            Some(Baked::Values(values)) => {
                if let Some(value) = values.get(&bits) {
                    return Ok(value.clone());
                }
            }
            Some(Baked::Failed(why)) => return Err(why.clone()),
            None => {}
        }
        if depth > MAX_DEPTH {
            return Err(format!(
                "expressions wait on one another more than {MAX_DEPTH} deep"
            ));
        }
        if !self.active.insert((node, bits)) {
            return Err("it is part of a cycle of expressions that read one another".into());
        }
        let result = self.run(node, time, depth);
        self.active.remove(&(node, bits));
        let mut baked = self.host.baked.borrow_mut();
        match &result {
            Ok(value) => {
                if let Baked::Values(values) = baked
                    .entry(node)
                    .or_insert_with(|| Baked::Values(HashMap::new()))
                {
                    values.insert(bits, value.clone());
                }
            }
            Err(why) => {
                baked.insert(node, Baked::Failed(why.clone()));
            }
        }
        result
    }

    /// Runs `node`'s expression at `time` until nothing it reads waits.
    fn run(&mut self, node: NodeId, time: f64, depth: usize) -> Result<Val, String> {
        let program = self.program(node)?;
        for _ in 0..MAX_WAITS {
            self.host.demand.set(None);
            match self.engine.run(program, node, time) {
                Ok(run) => {
                    if run.approximate {
                        self.approximate.insert(node);
                    }
                    return Ok(run.value);
                }
                Err(Failure::Pending(_)) => {
                    let Some((other, at)) = self.host.demand.take() else {
                        return Err("it waited on a value no read asked for".into());
                    };
                    if let Err(why) = self.evaluate(other, at, depth + 1) {
                        return Err(format!(
                            "it reads another expression, which could not be baked ({why})"
                        ));
                    }
                }
                Err(Failure::Unknown(what)) => {
                    return Err(format!("it reads {what}, which the import cannot know"));
                }
                Err(Failure::Error(error)) => return Err(format!("it fails: {error}")),
            }
        }
        Err(format!(
            "it waited on other expressions more than {MAX_WAITS} times"
        ))
    }

    /// `node`'s expression, compiled the first time it is needed.
    fn program(&mut self, node: NodeId) -> Result<Program, String> {
        if let Some(compiled) = self.programs.get(&node) {
            return compiled.clone();
        }
        let source = self.sources.get(&node).cloned().unwrap_or_default();
        let compiled = match self.engine.compile(&source, self.dialect) {
            Ok(compiled) => {
                if !compiled.repairs.is_empty() {
                    self.repairs.insert(node, compiled.repairs);
                }
                Ok(compiled.program)
            }
            Err(error) => Err(error.message),
        };
        self.programs.insert(node, compiled.clone());
        compiled
    }
}

/// The times of the frames `target`'s layer shows within `span` of its
/// comp, in the comp's seconds.
fn frames(target: &Target<'_>, (from, to): (f64, f64)) -> Result<Vec<f64>, String> {
    let fps = target.comp.frame_rate;
    if fps.is_nan() || fps <= 0.0 {
        return Err("its comp has no frame rate".into());
    }
    let start = target.layer.in_point.max(from).max(0.0);
    let end = target.layer.out_point.min(to).min(target.comp.duration);
    // Frames from the first the layer shows to the last before it ends.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..MAX_FRAMES"
    )]
    let (first, last) = (
        (start * fps - 1e-9).ceil().max(0.0) as usize,
        ((end * fps - 1e-9).ceil() - 1.0).max(0.0) as usize,
    );
    let last = last.max(first);
    if last - first >= MAX_FRAMES {
        return Err(format!("its layer shows more than {MAX_FRAMES} frames"));
    }
    #[expect(clippy::cast_precision_loss, reason = "frame numbers far below 2^52")]
    Ok((first..=last).map(|f| f as f64 / fps).collect())
}

/// The span of each comp's time the scene shows, by item id: the root's
/// whole duration, and in each comp a layer of it shows, the times the
/// layer shows it at, through its start and stretch. A layer that remaps
/// time may show any of its comp. Without a root, every comp's whole
/// duration.
fn spans(project: &Project, root: Option<u32>) -> HashMap<u32, (f64, f64)> {
    let comps: HashMap<u32, &Composition> = project
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Composition(comp) => Some((item.id, comp)),
            _ => None,
        })
        .collect();
    let Some(root) = root else {
        return comps
            .iter()
            .map(|(&id, c)| (id, (0.0, c.duration)))
            .collect();
    };
    let mut spans = HashMap::new();
    let Some(comp) = comps.get(&root) else {
        return spans;
    };
    spans.insert(root, (0.0, comp.duration));
    let mut queue = vec![root];
    // Each comp's span only grows, within its duration, so this ends; the
    // bound only guards against a bug.
    let mut rounds = 0;
    while let Some(id) = queue.pop() {
        rounds += 1;
        if rounds > 100_000 {
            break;
        }
        let (Some(&(from, to)), Some(comp)) = (spans.get(&id), comps.get(&id)) else {
            continue;
        };
        for layer in &comp.layers {
            let Some(inner) = layer.source.and_then(|s| comps.get(&s).map(|c| (s, c))) else {
                continue;
            };
            let (shown_from, shown_to) = (layer.in_point.max(from), layer.out_point.min(to));
            if shown_from >= shown_to {
                continue;
            }
            let remaps = layer
                .properties
                .properties()
                .any(|p| p.match_name == "ADBE Time Remapping" && remaps(p));
            let (a, b) = match remaps || layer.stretch == 0.0 {
                true => (0.0, inner.1.duration),
                false => {
                    let local = |t: f64| (t - layer.start_time) / layer.stretch;
                    let (a, b) = (local(shown_from), local(shown_to));
                    (a.min(b).max(0.0), a.max(b).min(inner.1.duration))
                }
            };
            if a >= b {
                continue;
            }
            let grown = match spans.get(&inner.0) {
                Some(&(x, y)) => (x.min(a), y.max(b)),
                None => (a, b),
            };
            if spans.get(&inner.0) != Some(&grown) {
                spans.insert(inner.0, grown);
                queue.push(inner.0);
            }
        }
    }
    spans
}

/// Whether a layer's time remapping property changes its time: it applies
/// and is keyed or has an expression; otherwise it holds its default.
fn remaps(property: &Property) -> bool {
    !property.hidden
        && (matches!(property.value, Stream::Animated(_))
            || property.expression.as_ref().is_some_and(|e| e.enabled))
}

/// The values an expression gave, as the property's own kind of value.
fn sampled(
    property: &Property,
    template: &Value,
    times: &[f64],
    values: &[Val],
) -> Result<Sampled, String> {
    if let Value::Text(_) = template {
        return Ok(Sampled::Text(values.iter().map(text_of).collect()));
    }
    let Some(own) = template.numbers() else {
        return Err("it sets a path or a value the bake does not take yet".into());
    };
    let unit = host::unit_of(property);
    let mut channels = vec![Vec::with_capacity(values.len()); own.len()];
    for (value, &time) in values.iter().zip(times) {
        let numbers = match value {
            Val::Number(n) => vec![*n],
            Val::Vector(v) => v.clone(),
            Val::Text(t) => {
                return Err(format!(
                    "it gives text ({t:?}) for a value that is not text"
                ));
            }
        };
        let own_now = property.value.value_at(time).numbers().unwrap_or_default();
        for (i, channel) in channels.iter_mut().enumerate() {
            let n = match numbers.get(i) {
                Some(n) if n.is_finite() => n / unit,
                Some(_) => return Err("it gives a value that is not a finite number".into()),
                None => own_now.get(i).copied().unwrap_or(0.0),
            };
            channel.push(n);
        }
    }
    let colour = matches!(template, Value::Color(_));
    let tolerance = if colour { 0.001 } else { 0.01 / unit };
    Ok(Sampled::Numbers {
        tolerances: vec![tolerance; channels.len()],
        channels,
    })
}

/// A value as source text shows it.
fn text_of(value: &Val) -> String {
    match value {
        Val::Text(t) => t.clone(),
        Val::Number(n) => number_text(*n),
        Val::Vector(v) => v
            .iter()
            .map(|n| number_text(*n))
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// A number as the language prints it, for the values it can take here.
fn number_text(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    format!("{n}")
}

#[cfg(test)]
#[path = "../../tests/unit/bake/mod.rs"]
mod tests;
