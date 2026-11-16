//! The bake itself: the engine run over the project, in order, at every
//! frame each expression's layer shows (see [`super`]).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use motion_expr::{Dialect, Engine, Failure, Limits, NodeId, Program, STACK_SIZE, Val};

use super::host::{Baked, ProjectHost, Target};
use super::{Bake, Bakes, MAX_DEPTH, Sampled, Samples, address, dialect};
use crate::model::{Composition, ItemKind, Project, Property, Stream, Value};
use crate::parts;

/// The most frames one expression is evaluated at.
const MAX_FRAMES: usize = 100_000;

/// How often one run may wait on another expression before it is stopped.
const MAX_WAITS: usize = 10_000;

/// [`super::bake`], with the engine.
pub(super) fn bake(project: &Project, root: Option<u32>) -> Bakes {
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
    let (selectors, targets): (Vec<&Target<'_>>, Vec<&Target<'_>>) = targets
        .iter()
        .partition(|t| t.property.match_name == parts::AMOUNT);
    let by_property = targets
        .iter()
        .map(|target| (address(target.property), baker.target(target)))
        .collect();
    let by_part = selectors
        .iter()
        .map(|target| (address(target.property), baker.parts(target)))
        .collect();
    Bakes {
        by_property,
        by_part,
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
            source,
            dialect: self.dialect,
            repairs: self.repairs.get(&target.node).cloned().unwrap_or_default(),
            outcome,
        }
    }

    /// An expression selector's amount, evaluated for each part of its
    /// layer's text at the frames its layer shows.
    fn parts(&mut self, target: &Target<'_>) -> Result<Vec<Samples>, String> {
        let counted = parts::count(&target.layer.properties, target.property)?;
        let total = u32::try_from(counted.total)
            .map_err(|_| "its text has more parts than the bake takes".to_owned())?;
        let samples = (1..=total)
            .map(|index| {
                self.engine.set_part(Some((index, total)));
                self.samples_of_part(target)
            })
            .collect();
        self.engine.set_part(None);
        samples
    }

    /// [`Baker::samples`] for one part of an expression selector: each run
    /// is the part's own, so none is remembered for other reads.
    fn samples_of_part(&mut self, target: &Target<'_>) -> Result<Samples, String> {
        let span = self
            .spans
            .get(&target.item)
            .copied()
            .unwrap_or((0.0, target.comp.duration));
        let times = frames(target, span)?;
        let mut values = Vec::with_capacity(times.len());
        let mut constant = false;
        for &time in &times {
            let (value, still) = self.run(target.node, time, 0)?;
            // One number chooses the part along every axis.
            let value = match value {
                Val::Number(n) => Val::Vector(vec![n; 3]),
                other => other,
            };
            values.push(value);
            if still {
                values.resize(times.len(), values[0].clone());
                constant = true;
                break;
            }
        }
        let template = target.property.value.value_at(times[0]);
        Ok(Samples {
            values: sampled(
                target.property,
                self.host.unit(target.node),
                &template,
                &times,
                &values,
            )?,
            times,
            approximate: self.approximate.contains(&target.node),
            constant,
        })
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
        let mut constant = false;
        for &time in &times {
            values.push(self.evaluate(target.node, time, 0)?);
            if let Some(Baked::Constant(value)) = self.host.baked.borrow().get(&target.node) {
                values.resize(times.len(), value.clone());
                constant = true;
                break;
            }
        }
        let template = target.property.value.value_at(times[0]);
        Ok(Samples {
            values: sampled(
                target.property,
                self.host.unit(target.node),
                &template,
                &times,
                &values,
            )?,
            times,
            approximate: self.approximate.contains(&target.node),
            constant,
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
            Some(Baked::Constant(value)) => return Ok(value.clone()),
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
        match result {
            Ok((value, true)) => {
                baked.insert(node, Baked::Constant(value.clone()));
                Ok(value)
            }
            Ok((value, false)) => {
                if let Baked::Values(values) = baked
                    .entry(node)
                    .or_insert_with(|| Baked::Values(HashMap::new()))
                {
                    values.insert(bits, value.clone());
                }
                Ok(value)
            }
            Err(why) => {
                baked.insert(node, Baked::Failed(why.clone()));
                Err(why)
            }
        }
    }

    /// Runs `node`'s expression at `time` until nothing it reads waits:
    /// its value, and whether it gives that value at every time.
    fn run(&mut self, node: NodeId, time: f64, depth: usize) -> Result<(Val, bool), String> {
        let program = self.program(node)?;
        for _ in 0..MAX_WAITS {
            self.host.demand.set(None);
            match self.engine.run(program, node, time) {
                Ok(run) => {
                    if run.approximate {
                        self.approximate.insert(node);
                    }
                    return Ok((run.value, run.constant));
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
    unit: super::host::Unit,
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
    let unit_of = |i: usize| unit.get(i).copied().unwrap_or(1.0);
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
            Val::Marker(_) => return Err("it gives a marker for a value that is not one".into()),
        };
        let own_now = property.value.value_at(time).numbers().unwrap_or_default();
        for (i, channel) in channels.iter_mut().enumerate() {
            let n = match numbers.get(i) {
                Some(n) if n.is_finite() => n / unit_of(i),
                Some(_) => return Err("it gives a value that is not a finite number".into()),
                None => own_now.get(i).copied().unwrap_or(0.0),
            };
            channel.push(n);
        }
    }
    let colour = matches!(template, Value::Color(_));
    let tolerances = (0..channels.len())
        .map(|i| if colour { 0.001 } else { 0.01 / unit_of(i) })
        .collect();
    Ok(Sampled::Numbers {
        tolerances,
        channels,
    })
}

/// A value as source text shows it.
fn text_of(value: &Val) -> String {
    match value {
        Val::Text(t) => t.clone(),
        Val::Marker(marker) => marker.comment.clone(),
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
#[path = "../../tests/unit/bake/run.rs"]
mod tests;
