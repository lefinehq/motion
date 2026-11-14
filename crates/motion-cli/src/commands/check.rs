//! `motion check`: print a scene's check report.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Result, Session};
use tracing::info;

use super::print_report;

/// Checks the scene file; exits 1 when the report has an error line.
pub fn run(path: &Path) -> Result<ExitCode> {
    let report = Session::open(path)?.check();
    info!(
        passed = report.passed,
        violations = report.violations.len(),
        "checked `{}`",
        path.display()
    );
    Ok(print_report(&report))
}
