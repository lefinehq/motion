//! The `motion` command-line tool, and the only place that prints: output to
//! stdout; failures, logs and timings to stderr. It exits 0, 1 when a check
//! has an error line, or a failure's [`Kind::exit_code`](motion_engine::Kind::exit_code).

mod commands;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use motion_engine::Quality;
use motion_engine::telemetry::timing::{REPORT_TARGET, Timings, millis};
use motion_engine::telemetry::{Format, LevelFilter, Options};
use tracing::info;

/// Check, query, edit and render motion scenes, and inspect `.aep` projects.
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

/// The `--quality` values.
#[derive(Clone, Copy, ValueEnum)]
enum QualityArg {
    /// 32-bit float compositing: no banding.
    Precise,
    /// 8- and 16-bit integer compositing: faster, for previews.
    Fast,
}

impl From<QualityArg> for Quality {
    fn from(quality: QualityArg) -> Self {
        match quality {
            QualityArg::Precise => Quality::Precise,
            QualityArg::Fast => Quality::Fast,
        }
    }
}

/// A subcommand; it converts to its name, as typed.
#[derive(Subcommand, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
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
    /// Draw frames of a scene into PNG files named `frame-00000.png`.
    Render {
        /// Path to the scene file.
        scene: PathBuf,
        /// The frames to draw, `A-B` with both ends included; all when
        /// neither this nor `--frame` is given.
        #[arg(long, value_name = "A-B", conflicts_with = "frame")]
        frames: Option<commands::render::Frames>,
        /// One frame to draw.
        #[arg(long, value_name = "N")]
        frame: Option<u32>,
        /// The folder the PNG files are written to; created if missing.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        /// How colours are computed: `precise` for delivered frames, `fast`
        /// for previews.
        #[arg(long, value_enum, default_value = "precise")]
        quality: QualityArg,
    },
    /// Print an `.aep` project: its items, layers and properties, then what
    /// the reader could not decode yet.
    Inspect {
        /// Path to the `.aep` file.
        project: PathBuf,
        /// Print the model and the report as JSON.
        #[arg(long)]
        json: bool,
    },
}

impl Command {
    /// The file the command works on.
    fn file(&self) -> &Path {
        match self {
            Command::Check { scene }
            | Command::At { scene, .. }
            | Command::Edit { scene, .. }
            | Command::Render { scene, .. } => scene,
            Command::Inspect { project, .. } => project,
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
    let command: &str = (&cli.command).into();
    info!(
        command,
        file = %cli.command.file().display(),
        "running"
    );
    let result = match cli.command {
        Command::Check { scene } => commands::check::run(&scene),
        Command::At { scene, time, ids } => commands::at::run(&scene, &time, &ids),
        Command::Edit { scene, line } => commands::edit::run(&scene, &line),
        Command::Render {
            scene,
            frames,
            frame,
            out,
            quality,
        } => commands::render::run(
            &scene,
            frames.or(frame.map(commands::render::Frames::one)),
            &out,
            quality.into(),
        ),
        Command::Inspect { project, json } => commands::inspect::run(&project, json),
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
