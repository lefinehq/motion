//! `motion-query`: ask a scene questions and get numbers back.
//!
//! This crate is a side branch of the pipeline, not a numbered rendering
//! step: it works on the output of step 3 (Evaluate). It sits on top of the
//! evaluated scene from `motion-eval` and answers the questions an agent
//! would otherwise need to look at frames for: [`check`] runs the standard
//! rules over every frame, and [`at`] prints where things are at one moment.
//! Everything is computed analytically; nothing is rendered.

mod at;
mod check;
mod report;
mod rules;

pub use at::at;
pub use check::check;
pub use report::{Report, Severity, Violation};
