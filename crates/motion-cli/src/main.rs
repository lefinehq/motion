//! The `motion` command-line tool: the only place that prints. It holds no
//! engine logic: every command goes through a `motion_engine::Session`,
//! which reads, runs and writes the scene (a side branch of the pipeline,
//! not a numbered rendering step).
//!
//! Commands:
//!
//! - `motion check <scene>` lists layout problems, then `OK n`.
//! - `motion at <scene> <time> [ids...]` prints what is on screen at a time.
//! - `motion edit <scene> <line>` changes the scene file, then prints its check.
//!
//! Exit codes: `0` when all is well, `1` when the check report has an error
//! line, and otherwise the code of the diagnostic's kind
//! ([`Kind::exit_code`](motion_engine::Kind::exit_code), `2` for every
//! kind today) when the file, the time or the edit line is unusable. Output
//! goes to stdout; failures print `error: {diagnostic}` to stderr, from here
//! only.
//!
//! Logs go to stderr through `motion-telemetry`: `--log <level>` (default
//! `off`, so a plain run's stderr is only the `error:` line; overridden by
//! `MOTION_LOG`), `--log-format text|json`, and `--timings` for the step
//! timings at the end: a `timings …` line, or one JSON event per step in
//! JSON mode.

mod commands;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use motion_engine::telemetry::timing::{REPORT_TARGET, Timings, millis};
use motion_engine::telemetry::{Format, LevelFilter, Options};
use tracing::info;

/// Check, query and edit motion scenes.
#[derive(Parser)]
#[command(name = "motion", version, about)]
struct Cli {
    /// The most verbose log level written to stderr: off, error, warn, info,
    /// debug or trace. `MOTION_LOG` (e.g. `motion_query=debug`) overrides it.
    #[arg(
        long = "log",
        value_name = "LEVEL",
        default_value = "off",
        global = true
    )]
    log: LevelFilter,
    /// How log lines are written to stderr.
    #[arg(long, value_enum, default_value = "text", global = true)]
    log_format: LogFormat,
    /// Print how long each step took to stderr, at the end: a `timings …`
    /// line, or one event per step in JSON log mode.
    #[arg(long, global = true)]
    timings: bool,
    /// The command to run.
    #[command(subcommand)]
    command: Command,
}

/// The `--log-format` values.
#[derive(Clone, Copy, ValueEnum)]
enum LogFormat {
    /// One human-readable line per event.
    Text,
    /// One JSON object per line, for machines and agents.
    Json,
}

impl From<LogFormat> for Format {
    fn from(format: LogFormat) -> Self {
        match format {
            LogFormat::Text => Format::Text,
            LogFormat::Json => Format::Json,
        }
    }
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

impl Command {
    /// The subcommand's name, as typed.
    fn name(&self) -> &'static str {
        match self {
            Command::Check { .. } => "check",
            Command::At { .. } => "at",
            Command::Edit { .. } => "edit",
        }
    }

    /// The scene file the command works on.
    fn scene(&self) -> &Path {
        match self {
            Command::Check { scene } | Command::At { scene, .. } | Command::Edit { scene, .. } => {
                scene
            }
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = cli.log_format.into();
    let telemetry = motion_engine::telemetry::init(Options {
        level: cli.log,
        format,
        timings: cli.timings,
    });
    info!(
        command = cli.command.name(),
        file = %cli.command.scene().display(),
        "running"
    );
    let result = match cli.command {
        Command::Check { scene } => commands::check::run(&scene),
        Command::At { scene, time, ids } => commands::at::run(&scene, &time, &ids),
        Command::Edit { scene, line } => commands::edit::run(&scene, &line),
    };
    let code = match result {
        Ok(code) => code,
        // The engine has already logged the diagnostic as an `error` event.
        Err(d) => {
            eprintln!("error: {d}");
            ExitCode::from(d.kind.exit_code())
        }
    };
    if let Some(timings) = telemetry.timings() {
        match format {
            Format::Text => eprintln!("{}", timings.report()),
            Format::Json => log_timings(timings),
        }
    }
    code
}

/// The timings as `info` events, one per step in the order of the text
/// line: `{"stage":"parse","ms":0.4}`. One event per step keeps the JSON log
/// stream one object per line, and the step name is a field value, so no
/// list of steps lives here.
fn log_timings(timings: &Timings) {
    for (step, total) in timings.steps() {
        info!(target: REPORT_TARGET, stage = %step, ms = millis(total), "timing");
    }
}
