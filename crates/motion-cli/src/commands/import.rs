//! `motion import`: import an `.aep` project as a scene, write it, and print
//! what the import mapped, defaulted, ignored or could not support.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Diagnostic, ImportOptions, Kind, Result, write_document};

/// Imports `project` with `options`, writes the scene to `out`, and prints
/// the mapping report, as JSON when `json` is set.
pub fn run(project: &Path, options: &ImportOptions, out: &Path, json: bool) -> Result<ExitCode> {
    let import = motion_engine::import_scene(project, options)?;
    fs::write(out, write_document(&import.json)).map_err(|e| Diagnostic::io("write", out, &e))?;
    if json {
        let text = serde_json::to_string_pretty(&import.mapping).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the report as JSON: {e}"),
            )
        })?;
        println!("{text}");
    } else {
        print!("{}", import.mapping);
        println!("  wrote {}", out.display());
    }
    Ok(ExitCode::SUCCESS)
}
