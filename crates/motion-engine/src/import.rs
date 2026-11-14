//! Reading an `.aep` project (steps `read` and `decode`), and importing it
//! as a scene (step `map`): the entry points for tools that import one.

use std::fs;
use std::path::Path;

use motion_format::{Diagnostic, Result};
use motion_import::Project;
use motion_import::map::{self, Import, Options};

use crate::stage::stage;

/// Reads the `.aep` project at `path` into its typed model (steps `read`
/// and `decode`).
///
/// # Errors
/// A [`Kind::Io`](motion_format::Kind::Io) diagnostic when the file cannot be
/// read, or the decoder's diagnostic, with its chunk path, when the file is
/// not a project it can follow.
pub fn import(path: impl AsRef<Path>) -> Result<Project> {
    let path = path.as_ref();
    let bytes = stage("read", || {
        fs::read(path).map_err(|e| Diagnostic::io("read", path, &e))
    })?;
    stage("decode", || motion_import::decode(&bytes))
}

/// Reads the `.aep` project at `path` and imports it as a scene (steps
/// `read`, `decode` and `map`), with what the import mapped, defaulted,
/// ignored or could not support.
///
/// # Errors
/// The diagnostics of [`import`], or a diagnostic when the composition to
/// import cannot be chosen.
pub fn import_scene(path: impl AsRef<Path>, options: &Options) -> Result<Import> {
    let project = import(path)?;
    stage("map", || map::import(&project, options))
}

#[cfg(test)]
#[path = "../tests/unit/import.rs"]
mod tests;
