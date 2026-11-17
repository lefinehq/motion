//! `nakkash export`: write an edited scene back into the `.aep` project it
//! was imported from, and print what was written back and what was not.

use std::path::Path;
use std::process::ExitCode;

use nakkash_engine::{Diagnostic, Kind, Result};

/// What to export.
pub struct Args<'a> {
    /// The edited scene.
    pub scene: &'a Path,
    /// The project it was imported from.
    pub project: &'a Path,
    /// Where to save the patched project.
    pub out: &'a Path,
    /// The composition it was imported from, when one was named then.
    pub comp: Option<String>,
    /// Print the report as JSON.
    pub json: bool,
}

/// Exports and prints the report.
pub fn run(args: Args<'_>) -> Result<ExitCode> {
    let plan = nakkash_engine::export_project(args.scene, args.project, args.out, args.comp)?;
    if args.json {
        let text = serde_json::to_string_pretty(&plan).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the report as JSON: {e}"),
            )
        })?;
        println!("{text}");
    } else {
        println!("exported `{}`", args.scene.display());
        print!("{plan}");
        println!(
            "  wrote {} ({} bytes patched)",
            args.out.display(),
            plan.patched_bytes()
        );
    }
    Ok(ExitCode::SUCCESS)
}
