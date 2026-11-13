//! `motion edit`: apply one edit line to a scene file, save it, then check it.
//!
//! Uses the `motion-edit` side branch on the raw document, writes the result
//! with `motion-format`, then runs the same check as `motion check`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use motion_edit::{apply_edit, parse_edit};
use motion_format::{scene_from_value, write_document};

use super::check::report;
use super::load;

/// Edits the scene file in place and prints the check report.
///
/// A scene that is valid but has check errors is still saved (exit 1);
/// format and edit errors leave the file untouched (exit 2).
pub fn run(path: &Path, line: &str) -> Result<ExitCode, String> {
    let doc = load(path)?;
    let edit = parse_edit(line).map_err(|e| e.to_string())?;
    let raw = apply_edit(&doc.raw, &edit).map_err(|e| e.to_string())?;
    let scene = scene_from_value(&raw).map_err(|e| e.to_string())?;
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
fn save(path: &Path, text: &str) -> Result<(), String> {
    let tmp = temp_path(path);
    let fail = |e: std::io::Error| format!("cannot write `{}`: {e}", path.display());
    std::fs::write(&tmp, text).map_err(fail)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        fail(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_path_is_a_sibling_with_a_suffix() {
        assert_eq!(
            temp_path(Path::new("dir/scene.json")),
            Path::new("dir/scene.json.tmp")
        );
        assert_eq!(
            temp_path(Path::new("scene.json")),
            Path::new("scene.json.tmp")
        );
    }
}
