//! `motion inspect`: print an `.aep` project as a readable tree followed by
//! its import report, or both as JSON.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Diagnostic, ImportReport, Kind, Project, Result};
use serde::Serialize;

/// What `--json` prints: the model as read, and its report.
#[derive(Serialize)]
struct Inspection<'a> {
    project: &'a Project,
    report: &'a ImportReport,
}

/// Reads the project and prints it; `json` prints the model instead of the tree.
pub fn run(path: &Path, json: bool) -> Result<ExitCode> {
    let project = motion_engine::import(path)?;
    let report = project.report();
    if json {
        let inspection = Inspection {
            project: &project,
            report: &report,
        };
        let text = serde_json::to_string_pretty(&inspection).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the project as JSON: {e}"),
            )
        })?;
        println!("{text}");
    } else {
        print!("{project}{report}");
    }
    Ok(ExitCode::SUCCESS)
}
