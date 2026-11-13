//! The commands behind the `motion` binary, plus the steps they share:
//! loading a scene file and compiling it into a scene graph.
//!
//! The command layer of the CLI: the only code in the workspace that touches
//! files. It runs steps 1 to 3 (read, compile, evaluate) and hands the result
//! to the query and edit side branch.

pub mod at;
pub mod check;
pub mod edit;

use std::path::Path;

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_format::{Document, Scene, parse_document};
use motion_scene::{ApproxTextMeasure, SceneGraph, compile};

/// Reads and validates a scene file.
pub fn load(path: &Path) -> Result<Document> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Diagnostic::new(Kind::Io, format!("cannot read `{}`: {e}", path.display())))?;
    parse_document(&text)
}

/// Compiles a scene with the built-in approximate text measurer.
pub fn compile_scene(scene: &Scene) -> SceneGraph {
    compile(scene, &ApproxTextMeasure)
}
