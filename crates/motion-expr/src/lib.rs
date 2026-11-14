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
//! [D38]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d38-expressions-are-baked-at-import

mod engine;
pub mod fit;
mod host;
mod noise;
mod rewrite;

pub use engine::{Compiled, Engine, Failure, Limits, Program, Run, STACK_SIZE};
pub use host::{Host, Missing, NodeId, NodeKind, Select, Val};
pub use rewrite::{Dialect, Rewritten, rewrite};
