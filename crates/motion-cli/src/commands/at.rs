//! `motion at`: print where layers are at one moment in time.
//!
//! Loads and compiles the scene (steps 1 and 2), then asks `motion-query`
//! for the evaluated values (step 3) at one time.

use std::path::Path;
use std::process::ExitCode;

use motion_diagnostic::Result;
use motion_format::units::Time;
use motion_query::at;

use super::{compile_scene, load};

/// Prints the layers named in `ids` (all of them when empty) at `time`.
pub fn run(path: &Path, time: &str, ids: &[String]) -> Result<ExitCode> {
    let doc = load(path)?;
    // The time may be seconds, so it needs the scene's frame rate.
    let frame = time.parse::<Time>()?.to_frames(doc.scene.fps);
    let graph = compile_scene(&doc.scene);
    let text = at(&graph, frame, ids)?;
    println!("{text}");
    Ok(ExitCode::SUCCESS)
}
