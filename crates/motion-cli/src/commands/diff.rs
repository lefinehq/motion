//! `motion diff`: compare a scene with a reference render's frames, print
//! the pooled error, and write every frame's numbers as JSON.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Diagnostic, Diff, DiffOptions, Kind, Pooled, Result, Session};

/// What `motion diff` takes besides the scene.
pub struct Args<'a> {
    /// The folder of reference frames.
    pub reference: &'a Path,
    /// The reference's frame rate.
    pub fps: f64,
    /// The scene time of the first reference frame, in seconds.
    pub start: f64,
    /// The per-channel tolerance, 0 to 1.
    pub tolerance: f64,
    /// Where to write the numbers as JSON, if anywhere.
    pub json: Option<&'a Path>,
    /// Where to write the error images, if anywhere.
    pub images: Option<&'a Path>,
    /// Layers left out of the drawing and the measure, by id: what the
    /// reference shows differently on purpose, such as a placeholder.
    pub ignore: &'a [String],
}

/// Compares `scene` with the reference and prints the summary.
pub fn run(scene: &Path, args: &Args<'_>) -> Result<ExitCode> {
    let mut session = Session::open(scene)?;
    for id in args.ignore {
        session.edit(&format!("{id} hidden=true"))?;
    }
    let options = DiffOptions {
        frames: DiffOptions::frames_in(args.reference)?,
        fps: args.fps,
        start: args.start,
        tolerance: args.tolerance,
        images: args.images.map(Path::to_path_buf),
    };
    let diff = session.diff(&options)?;
    print!("{}", summary(&diff));
    if let Some(path) = args.json {
        let text = serde_json::to_string_pretty(&diff).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the numbers as JSON: {e}"),
            )
        })?;
        fs::write(path, text).map_err(|e| Diagnostic::io("write", path, &e))?;
        println!("  wrote {}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// The pooled numbers, one line each.
fn summary(diff: &Diff) -> String {
    let line = |name: &str, pooled: &Pooled| {
        format!(
            "  {name}: mean {:.4}, p99 {:.4}, worst {:.4} ({})\n",
            pooled.mean, pooled.p99, pooled.worst, pooled.worst_frame
        )
    };
    format!(
        "{} frames, tolerance {}\n{}{}{}{}",
        diff.frames.len(),
        diff.tolerance,
        line("error where drawn", &diff.drawn_mean),
        line("failing where drawn", &diff.drawn_failing),
        line("error over all drawn", &diff.union_mean),
        line("failing over all drawn", &diff.union_failing),
    )
}
