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
//!   that the scene shows for 30 seconds is evaluated for those 30. A run
//!   that read nothing that changes with time gives its value at every
//!   frame, so such an expression runs once.
//! - **Layout.** `sourceRectAtTime` reads the rectangle a layer's content
//!   covers, which only the scene's own layout knows: its text set in its
//!   fonts, its shapes drawn. The bake answers it from [`Layouts`] measured
//!   in the scene an earlier pass wrote. A rectangle not measured yet is
//!   asked for ([`Bakes::asks`]); the expression that read it is not baked
//!   in this pass, but runs on at its other frames, so one pass asks for
//!   every rectangle its frames read. The caller measures them and imports
//!   again ([`crate::map::import`]).
//! - **Threads.** The engine recurses, so the bake runs on a thread with
//!   `motion_expr::STACK_SIZE` of stack.
//! - **Builds.** Evaluating needs the JavaScript engine, which only the
//!   `bake` feature links. Without it every expression is reported as not
//!   baked ([`OFF`]) and the keys under it stay, so a crate that renders
//!   but does not import links no script engine (D38).
//! - **Expression selectors.** The amount of an expression selector is
//!   evaluated once per part of its layer's text, with `textIndex` and
//!   `textTotal` set ([`Bakes::parts`], [D64]).
//!
//! [D38]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d38-expressions-are-baked-at-import
//! [D64]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d64-a-selector-may-choose-each-part-by-its-own-amount

use std::collections::{BTreeMap, BTreeSet, HashMap};

use motion_expr::Dialect;

use crate::model::{ExpressionEngine, Project, Property};

#[cfg(feature = "bake")]
mod host;
#[cfg(feature = "bake")]
mod run;

/// How deep expressions may wait on one another.
pub const MAX_DEPTH: usize = 64;

/// Why nothing is baked in a build without the `bake` feature.
pub const OFF: &str = "this build does not evaluate expressions (the `bake` feature is off)";

/// Why an expression that read a rectangle not measured yet is not baked.
pub const UNMEASURED: &str = "it reads the size of a layer's content, which was not measured";

/// The rectangle a layer's content covers, as an expression asks for it:
/// `sourceRectAtTime(time, extents)` on a text or shape layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ask {
    /// The item id of the layer's composition.
    pub comp: u32,
    /// The layer's id.
    pub layer: u32,
    /// The time, in the composition's seconds.
    pub time: f64,
    /// Whether strokes and text boxes count.
    pub extents: bool,
}

/// What tells [`Ask`]s apart: the comp, the layer, the time's bits and
/// the extents.
pub(crate) type AskKey = (u32, u32, u64, bool);

impl Ask {
    /// What tells asks apart: the same layer, time and extents.
    pub(crate) fn key(&self) -> AskKey {
        (self.comp, self.layer, self.time.to_bits(), self.extents)
    }
}

/// Rectangles measured for [`Ask`]s, `[left, top, width, height]` in each
/// layer's own coordinates, or why one could not be measured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layouts {
    measured: BTreeMap<AskKey, Result<[f64; 4], String>>,
}

impl Layouts {
    /// What was measured for `ask`, if it was.
    pub fn get(&self, ask: &Ask) -> Option<&Result<[f64; 4], String>> {
        self.measured.get(&ask.key())
    }

    /// Keeps what was measured for `ask`.
    pub fn insert(&mut self, ask: &Ask, rect: Result<[f64; 4], String>) {
        self.measured.insert(ask.key(), rect);
    }

    /// How many asks were measured.
    pub fn len(&self) -> usize {
        self.measured.len()
    }

    /// Whether none was.
    pub fn is_empty(&self) -> bool {
        self.measured.is_empty()
    }
}

/// Every expression of a project, evaluated.
#[derive(Debug, Default)]
pub struct Bakes {
    /// Each property's bake, by the property's address in the project.
    by_property: HashMap<usize, Bake>,
    /// Each expression selector's amount, baked once per part of its text,
    /// by the amount's address ([`Bakes::parts`]).
    by_part: HashMap<usize, Result<Vec<Samples>, String>>,
    /// Why nothing was baked, when the engine could not run at all.
    pub stopped: Option<String>,
    /// The rectangles read that [`Layouts`] did not have, in the order
    /// first read, each once.
    pub asks: Vec<Ask>,
    /// The layers, by comp item id and layer id, whose content (text or
    /// shapes) has an expression not baked for want of a rectangle: what
    /// they draw may change once it is measured.
    pub unsettled: BTreeSet<(u32, u32)>,
}

impl Bakes {
    /// The bake of `property`, if it has an expression that is switched on
    /// and the engine ran.
    pub fn get(&self, property: &Property) -> Option<&Bake> {
        self.by_property.get(&address(property))
    }

    /// The samples of an expression selector's `amount`, one list per part
    /// of its text in reading order, `textIndex` running from 1; or why they
    /// could not be taken. `None` when the bake did not reach it.
    pub fn parts(&self, amount: &Property) -> Option<&Result<Vec<Samples>, String>> {
        self.by_part.get(&address(amount))
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
    /// The expression's source, whole, as the project stores it.
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
    /// Whether it read nothing that changes with time, so it ran once and
    /// its value is the same at every frame.
    pub constant: bool,
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
pub(crate) fn address(property: &Property) -> usize {
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
/// its whole duration without one. Without the `bake` feature, nothing is
/// evaluated and [`Bakes::stopped`] says why. No layer's rectangle is
/// measured: every one read is asked for ([`bake_with`]).
pub fn bake(project: &Project, root: Option<u32>) -> Bakes {
    bake_with(project, root, &Layouts::default())
}

/// [`bake`], reading layers' rectangles from `layouts`.
pub fn bake_with(project: &Project, root: Option<u32>, layouts: &Layouts) -> Bakes {
    #[cfg(feature = "bake")]
    return run::bake_all(project, root, layouts);
    #[cfg(not(feature = "bake"))]
    {
        let _ = (project, root, layouts);
        Bakes {
            stopped: Some(OFF.into()),
            ..Bakes::default()
        }
    }
}
