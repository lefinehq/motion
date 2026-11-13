//! `motion edit`: apply one edit line to a scene file, save it, then check it.
//!
//! Uses the `motion-edit` side branch on the raw document, writes the result
//! with `motion-format`, then runs the same check as `motion check`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_edit::{apply_edit, parse_edit};
use motion_format::{scene_from_value, write_document};

use super::check::report;
use super::load;

/// Edits the scene file in place and prints the check report.
///
/// A scene that is valid but has check errors is still saved (exit 1);
/// format and edit errors leave the file untouched (exit 2).
pub fn run(path: &Path, line: &str) -> Result<ExitCode> {
    let doc = load(path)?;
    let edit = parse_edit(line)?;
    let raw = apply_edit(&doc.raw, &edit)?;
    let scene = scene_from_value(&raw)?;
    save(path, &write_document(&raw))?;
    Ok(report(&scene))
}

/// The sibling file the new text is written to before it replaces `path`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes `text` to a sibling temp file and renames it over `path`.
///
/// A rename is atomic, so a failed write never leaves a half-written scene.
fn save(path: &Path, text: &str) -> Result<()> {
    let tmp = temp_path(path);
    let fail = |e: std::io::Error| {
        Diagnostic::new(Kind::Io, format!("cannot write `{}`: {e}", path.display()))
    };
    std::fs::write(&tmp, text).map_err(fail)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        fail(e)
    })
}

#[cfg(test)]
#[path = "../../tests/unit/commands/edit.rs"]
mod tests;
