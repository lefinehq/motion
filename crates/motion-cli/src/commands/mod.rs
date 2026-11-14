//! The commands behind the `motion` binary, plus the report printing they
//! share.
//!
//! The command layer of the CLI: each command opens a
//! [`Session`](motion_engine::Session), which runs the pipeline, and prints
//! what it returns. No command calls a pipeline crate directly.

pub mod at;
pub mod check;
pub mod edit;

use std::process::ExitCode;

use motion_engine::Report;

/// Prints a check report and maps it to an exit code: 1 when it has an
/// error line.
pub fn print_report(report: &Report) -> ExitCode {
    println!("{report}");
    if report.has_errors() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
