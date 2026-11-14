//! The commands behind the `motion` binary; each one opens a
//! [`Session`](motion_engine::Session) and prints what it returns.

pub mod at;
pub mod check;
pub mod diff;
pub mod edit;
pub mod fidelity;
pub mod fonts;
pub mod import;
pub mod inspect;
pub mod render;

use std::process::ExitCode;

use motion_engine::Report;

/// Prints a check report; exits 1 when it has an error line.
pub fn print_report(report: &Report) -> ExitCode {
    println!("{report}");
    if report.has_errors() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
