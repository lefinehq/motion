//! `motion-query`: ask a scene questions and get numbers back.
//!
//! This crate is step 4 of the pipeline (Query). It sits on top of the
//! evaluated scene from `motion-eval` and answers the questions an agent
//! would otherwise need to look at frames for: [`check`] runs the standard
//! rules over every frame, and [`at`] prints where things are at one moment.
//! Everything is computed analytically; nothing is rendered.

mod at;
mod check;
mod report;
mod rules;

pub use at::{QueryError, at};
pub use check::check;
pub use report::{Report, Severity, Violation};
