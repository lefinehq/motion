//! `nakkash check`: print a scene's check report.

use std::path::Path;
use std::process::ExitCode;

use nakkash_engine::{Report, Result, Session};

use super::print_new;

/// Checks the scene file, with the pixel gates after the scene's rules;
/// exits 1 when the report has an error line. A template's own values
/// that break its slots' contracts are printed first, as warnings. With a `baseline`, an earlier
/// version of the scene, only what the baseline does not already have is
/// printed and counted.
pub fn run(path: &Path, baseline: Option<&Path>) -> Result<ExitCode> {
    let session = Session::open(path)?;
    for problem in nakkash_engine::instance::own_problems(&session) {
        println!("{problem}");
    }
    let report = session.full_check()?;
    let base = baseline.map(full).transpose()?.unwrap_or_default();
    Ok(print_new(report, &base))
}

/// The scene's rules, then its pixel gates.
fn full(path: &Path) -> Result<Report> {
    Session::open(path)?.full_check()
}
