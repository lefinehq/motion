//! `nakkash edit`: apply edit lines to a scene file, then print what they
//! added to the scene's check.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use nakkash_engine::{Result, Session};

use super::print_new;

/// Edits the scene file in place, applying `lines` in order, and prints
/// what they added to the check report: what the scene already had before
/// is counted, not printed. A valid scene with new check errors is still
/// saved (exit 1); other errors leave it untouched.
pub fn run(path: &Path, lines: &[String]) -> Result<ExitCode> {
    let mut session = Session::open(path)?;
    // Kept, not copied: the check after the edit starts from it (D79).
    let before = Arc::clone(session.checked()?);
    let (last, rest) = lines.split_last().expect("clap asks for one line");
    for line in rest {
        session.edit(line)?;
    }
    let report = session.edit(last)?;
    session.save()?;
    Ok(print_new(report, before.report()))
}
