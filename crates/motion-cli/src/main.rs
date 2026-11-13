//! The `motion` command-line tool: the only place that reads and writes files
//! and prints. It holds no engine logic; it wires `motion-format`,
//! `motion-scene`, `motion-eval`, `motion-query` and `motion-edit` together
//! (a side branch of the pipeline, not a numbered rendering step).
//!
//! Commands:
//!
//! - `motion check <scene>` lists layout problems, then `OK n`.
//! - `motion at <scene> <time> [ids...]` prints what is on screen at a time.
//! - `motion edit <scene> <line>` changes the scene file, then prints its check.
//!
//! Exit codes: `0` when all is well, `1` when the check report has an error
//! line, and otherwise the code of the diagnostic's kind
//! ([`Kind::exit_code`](motion_diagnostic::Kind::exit_code), `2` for every
//! kind today) when the file, the time or the edit line is unusable. Output
//! goes to stdout; failures print `error: {diagnostic}` to stderr, from here
//! only.

mod commands;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Check, query and edit motion scenes.
#[derive(Parser)]
#[command(name = "motion", version, about)]
struct Cli {
    /// The command to run.
    #[command(subcommand)]
    command: Command,
}

/// The commands `motion` understands.
#[derive(Subcommand)]
enum Command {
    /// Check a scene for problems such as layers leaving the frame; exits 1 on errors.
    Check {
        /// Path to the scene file.
        scene: PathBuf,
    },
    /// Show where layers are at one moment in time.
    At {
        /// Path to the scene file.
        scene: PathBuf,
        /// The moment, as seconds or frames, for example `1.5s` or `45f`.
        time: String,
        /// Layer ids to show; every layer is shown when none are given.
        ids: Vec<String>,
    },
    /// Apply an edit line such as `card fill=#FF0000` and save the scene.
    Edit {
        /// Path to the scene file.
        scene: PathBuf,
        /// The edit line: a layer id (or `scene`) followed by `key=value` pairs.
        line: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check { scene } => commands::check::run(&scene),
        Command::At { scene, time, ids } => commands::at::run(&scene, &time, &ids),
        Command::Edit { scene, line } => commands::edit::run(&scene, &line),
    };
    match result {
        Ok(code) => code,
        Err(d) => {
            eprintln!("error: {d}");
            ExitCode::from(d.kind.exit_code())
        }
    }
}
