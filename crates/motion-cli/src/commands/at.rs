//! `motion at`: print where layers are at one moment in time.

use std::path::Path;
use std::process::ExitCode;

use motion_format::units::Time;
use motion_query::at;

use super::{compile_scene, load};

/// Prints the layers named in `ids` (all of them when empty) at `time`.
pub fn run(path: &Path, time: &str, ids: &[String]) -> Result<ExitCode, String> {
    let doc = load(path)?;
    // The time may be seconds, so it needs the scene's frame rate.
    let frame = Time::parse(time)
        .map_err(|e| e.to_string())?
        .to_frames(doc.scene.fps);
    let graph = compile_scene(&doc.scene);
    let text = at(&graph, frame, ids).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(ExitCode::SUCCESS)
}
