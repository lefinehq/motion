//! Reading an `.aep` project (steps `read` and `decode`), the entry point for
//! tools that import one.

use std::fs;
use std::path::Path;

use motion_format::{Diagnostic, Result};
use motion_import::Project;

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

#[cfg(test)]
#[path = "../tests/unit/import.rs"]
mod tests;
