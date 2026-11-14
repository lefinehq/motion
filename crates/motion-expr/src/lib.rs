//! Expressions of imported projects, baked at import ([D38]).
//!
//! - [`rewrite`] prepares a source: arithmetic becomes calls to helpers
//!   that follow the language's array rules, and legacy-engine sources get
//!   the forms that engine accepted.
//! - An [`Engine`] compiles sources once and runs them at any time over a
//!   [`Host`], the importer's view of the project, with the language's
//!   objects and functions. Runs are deterministic: no clock, our own
//!   randomness, and maths that gives the same bits on every platform.
//! - [`fit`] turns the values sampled at every frame back into a few
//!   editable keys within a tolerance.
//!
//! The engine, and the rewrite it runs on, sit behind the `engine` feature
//! (on by default). Without it the crate holds only what needs no engine:
//! the [`Host`] vocabulary, [`Dialect`] and [`fit`].
//!
//! [D38]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d38-expressions-are-baked-at-import

#[cfg(feature = "engine")]
mod engine;
pub mod fit;
mod host;
#[cfg(feature = "engine")]
mod noise;
#[cfg(feature = "engine")]
mod rewrite;

#[cfg(feature = "engine")]
pub use engine::{Compiled, Engine, Failure, Limits, Program, Run, STACK_SIZE};
pub use host::{Host, Missing, NodeId, NodeKind, Select, Val};
#[cfg(feature = "engine")]
pub use rewrite::{Rewritten, rewrite};

/// The engine a project evaluates its expressions with, which decides the
/// forms a source may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// The modern engine: ECMAScript 2018.
    Modern,
    /// The legacy engine: ECMAScript 3, with `if … else` on one line, the
    /// snake_case aliases and `this(…)` for `thisLayer(…)`.
    Legacy,
}
