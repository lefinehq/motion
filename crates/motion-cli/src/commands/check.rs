//! `motion check`: print a scene's check report.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Result, Session};

use super::print_report;

/// Checks the scene file; exits 1 when the report has an error line.
pub fn run(path: &Path) -> Result<ExitCode> {
    Ok(print_report(&Session::open(path)?.check()?))
}
