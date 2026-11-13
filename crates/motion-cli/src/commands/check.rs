//! `motion check`: compile a scene and print its check report.

use std::path::Path;
use std::process::ExitCode;

use motion_format::Scene;
use motion_query::check;

use super::{compile_scene, load};

/// Checks the scene file; exits 1 when the report has an error line.
pub fn run(path: &Path) -> Result<ExitCode, String> {
    let doc = load(path)?;
    Ok(report(&doc.scene))
}

/// Prints the check report for a scene and maps it to an exit code.
pub fn report(scene: &Scene) -> ExitCode {
    let report = check(scene, &compile_scene(scene));
    println!("{report}");
    if report.has_errors() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
