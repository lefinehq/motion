//! The commands behind the `motion` binary, plus the steps they share:
//! loading a scene file and compiling it into a scene graph.

pub mod at;
pub mod check;
pub mod edit;

use std::path::Path;

use motion_format::{Document, Scene, parse_document};
use motion_scene::{ApproxTextMeasure, SceneGraph, compile};

/// Reads and validates a scene file; the error is ready to print.
pub fn load(path: &Path) -> Result<Document, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
    parse_document(&text).map_err(|e| e.to_string())
}

/// Compiles a scene with the built-in approximate text measurer.
pub fn compile_scene(scene: &Scene) -> SceneGraph {
    compile(scene, &ApproxTextMeasure)
}
