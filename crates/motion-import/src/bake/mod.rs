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
//!   `motion_expr::STACK_SIZE` of stack.
//! - **Builds.** Evaluating needs the JavaScript engine, which only the
//!   `bake` feature links. Without it every expression is reported as not
//!   baked ([`OFF`]) and the keys under it stay, so a crate that renders
//!   but does not import links no script engine (D38).
//!
//! [D38]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d38-expressions-are-baked-at-import

use std::collections::HashMap;

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
/// evaluated and [`Bakes::stopped`] says why.
pub fn bake(project: &Project, root: Option<u32>) -> Bakes {
    #[cfg(feature = "bake")]
    return run::bake(project, root);
    #[cfg(not(feature = "bake"))]
    {
        let _ = (project, root);
        Bakes {
            stopped: Some(OFF.into()),
            ..Bakes::default()
        }
    }
}
