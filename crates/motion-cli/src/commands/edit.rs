//! `motion edit`: apply one edit line to a scene file, save it, then print
//! its check report.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Result, Session};

use super::print_report;

/// Edits the scene file in place and prints the check report.
///
/// A scene that is valid but has check errors is still saved (exit 1);
/// format and edit errors leave the file untouched (exit 2).
pub fn run(path: &Path, line: &str) -> Result<ExitCode> {
    let mut session = Session::open(path)?;
    let report = session.edit(line)?;
    session.save()?;
    Ok(print_report(&report))
}
