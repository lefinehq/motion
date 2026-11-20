//! Scenes whose compositions are in files of their own ([D91]): read
//! flattened, never saved over, and written split on request.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::fs;
use std::path::Path;

use nakkash_format::include;
use nakkash_format::{Diagnostic, Kind, Result};
use serde_json::Value as Json;

/// The scene text `text`, read from `path`, with each included composition
/// read from its file, relative to the scene file's folder; `text` as it
/// is when it includes none.
///
/// # Errors
/// A [`Kind::Io`] diagnostic for a file that cannot be read, or the
/// format's for an include that does not read, at `comps.<id>.include`.
pub(crate) fn flattened(path: &Path, text: String) -> Result<String> {
    if !text.contains("\"include\"") {
        return Ok(text);
    }
    let Ok(raw) = serde_json::from_str::<Json>(&text) else {
        // Not JSON: the parse reports it.
        return Ok(text);
    };
    if !include::has_includes(&raw) {
        return Ok(text);
    }
    let folder = path.parent().unwrap_or(Path::new(""));
    let flat = include::flatten(&raw, |file| {
        let file = folder.join(file);
        fs::read_to_string(&file).map_err(|e| Diagnostic::io("read", &file, &e))
    })?;
    Ok(flat.to_string())
}

/// Whether the file at `path` is a scene split into files.
pub(crate) fn is_split(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .filter(|text| text.contains("\"include\""))
        .and_then(|text| serde_json::from_str::<Json>(&text).ok())
        .is_some_and(|raw| include::has_includes(&raw))
}

/// The refusal to save over a split scene.
pub(crate) fn refused(path: &Path) -> Diagnostic {
    Diagnostic::new(
        Kind::Conflict,
        format!(
            "`{}` keeps its compositions in files of their own; saving it as one file would undo that",
            path.display()
        ),
    )
    .expected("an edit of the composition's own file, or `nakkash flatten` first and an edit of the flat scene")
}
