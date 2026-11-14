//! `motion at`: print where layers are at one moment in time.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Result, Session};

/// Prints the layers named in `ids` (all of them when empty) at `time`.
pub fn run(path: &Path, time: &str, ids: &[String]) -> Result<ExitCode> {
    let text = Session::open(path)?.at(time, ids)?;
    println!("{text}");
    Ok(ExitCode::SUCCESS)
}
