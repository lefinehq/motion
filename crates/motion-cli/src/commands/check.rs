//! `motion check`: print a scene's check report.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Result, Session};

use super::print_report;

/// Checks the scene file, with the pixel gates after the scene's rules;
/// exits 1 when the report has an error line.
pub fn run(path: &Path) -> Result<ExitCode> {
    let session = Session::open(path)?;
    let mut report = session.check()?;
    report.join(session.gates()?);
    Ok(print_report(&report))
}
