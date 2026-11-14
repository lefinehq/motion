//! The wrapper every pipeline step runs through.
//!
//! [`stage`] opens a `debug` span named `stage` with the step's name in its
//! `stage` field. The span is what times the step: `motion-telemetry`'s
//! timings layer measures every span with a `stage` field from creation to
//! close, whatever the log level, and keys it by that field. (A span's name
//! is fixed where it is written, so the step's name goes in a field.) The
//! level is `debug` because a step is a debug-level detail; `info` is one
//! line per command. A failing step is logged here, once, as an `error`
//! event with the diagnostic's kind, path and expected form; no step logs
//! or maps its own errors.

use motion_diagnostic::{Diagnostic, Result};
use tracing::{debug_span, error};

/// Runs the step `name`, logging its diagnostic, if any, before returning it
/// unchanged.
///
/// # Errors
/// Returns the step's own diagnostic.
pub fn stage<T>(name: &'static str, step: impl FnOnce() -> Result<T>) -> Result<T> {
    in_span(name, || step().inspect_err(log))
}

/// Runs the step `name`, which cannot fail, in the same span as [`stage`].
pub fn stage_ok<T>(name: &'static str, step: impl FnOnce() -> T) -> T {
    in_span(name, step)
}

fn in_span<T>(name: &'static str, step: impl FnOnce() -> T) -> T {
    let _span = debug_span!("stage", stage = name).entered();
    step()
}

/// The one `error` event of a failed step, inside the step's span.
fn log(d: &Diagnostic) {
    error!(
        kind = ?d.kind,
        path = d.path.as_deref(),
        expected = d.expected.as_deref(),
        "{}",
        d.message
    );
}

#[cfg(test)]
#[path = "../tests/unit/stage.rs"]
mod tests;
