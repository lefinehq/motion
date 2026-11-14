//! The `motion` command-line tool, and the only place that prints: output to
//! stdout; failures, logs and timings to stderr. It exits 0, 1 when a check
//! has an error line, or a failure's [`Kind::exit_code`](motion_engine::Kind::exit_code).

mod commands;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use motion_engine::telemetry::timing::{REPORT_TARGET, Timings, millis};
use motion_engine::telemetry::{Format, LevelFilter, Options};
use tracing::info;

/// Check, query and edit motion scenes.
#[derive(Parser)]
#[command(name = "motion", version)]
struct Cli {
    /// Log level on stderr: off, error, warn, info, debug or trace.
    /// `MOTION_LOG` (e.g. `motion_query=debug`) overrides it.
    #[arg(
        long = "log",
        value_name = "LEVEL",
        default_value = "off",
        global = true
    )]
    log: LevelFilter,
    /// How log lines are written.
    #[arg(long, value_enum, default_value = "text", global = true)]
    log_format: LogFormat,
    /// Print each step's time to stderr at the end.
    #[arg(long, global = true)]
    timings: bool,
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

#[derive(Subcommand)]
enum Command {
    /// Check a scene for problems such as layers leaving the frame; exits 1 on errors.
    Check {
        /// Path to the scene file.
        scene: PathBuf,
    },
    /// Show where layers are at one moment.
    At {
        /// Path to the scene file.
        scene: PathBuf,
        /// The moment, such as `1.5s` or `45f`.
        time: String,
        /// Layer ids to show; all when none are given.
        ids: Vec<String>,
    },
    /// Apply an edit line such as `card fill=#FF0000` and save the scene.
    Edit {
        /// Path to the scene file.
        scene: PathBuf,
        /// A layer id (or `scene`) followed by `key=value` pairs.
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
        // The engine has already logged it.
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

/// The timings as one `info` event per step, so the JSON log stays one
/// object per line.
fn log_timings(timings: &Timings) {
    for (step, total) in timings.steps() {
        info!(target: REPORT_TARGET, stage = %step, ms = millis(total), "timing");
    }
}
