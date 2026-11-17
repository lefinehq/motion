//! The wrapper every step of a [`Session`](crate::Session) runs through: the
//! span that times the step, and the one log of its failure.

use nakkash_format::{Diagnostic, Result};
use tracing::error;

use crate::telemetry::timing::timed;

/// Runs the step `name`, logging its diagnostic, if any, before returning it.
pub(crate) fn stage<T>(name: &'static str, step: impl FnOnce() -> Result<T>) -> Result<T> {
    in_span(name, || step().inspect_err(log))
}

/// Runs the step `name`, which cannot fail.
pub(crate) fn stage_ok<T>(name: &'static str, step: impl FnOnce() -> T) -> T {
    in_span(name, step)
}

/// Runs `step` in the span the timings recognise.
fn in_span<T>(name: &'static str, step: impl FnOnce() -> T) -> T {
    timed(name, step)
}

/// The one `error` event of a failed step.
fn log(d: &Diagnostic) {
    error!(
        kind = ?d.kind,
        path = d.path_text().as_deref(),
        expected = d.expected.as_deref(),
        "{}",
        d.message
    );
}

#[cfg(test)]
#[path = "../tests/unit/stage.rs"]
mod tests;
