//! `motion edit`: apply one edit line to a scene file, then print what it
//! added to the scene's check.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Result, Session};

use super::print_new;

/// Edits the scene file in place and prints what the edit added to the check
/// report: what the scene already had before the edit is counted, not
/// printed. A valid scene with new check errors is still saved (exit 1);
/// other errors leave it untouched.
pub fn run(path: &Path, line: &str) -> Result<ExitCode> {
    let mut session = Session::open(path)?;
    let before = session.check()?;
    let report = session.edit(line)?;
    session.save()?;
    Ok(print_new(report, &before))
}
