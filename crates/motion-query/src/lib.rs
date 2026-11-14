//! Answers questions about an evaluated scene with numbers instead of frames: [`check`]
//! runs the standard rules, [`at`] says where things are at one moment.

mod at;
mod check;
mod report;
mod rules;

pub use at::at;
pub use check::check;
pub use report::{Code, Report, Severity, Violation};
