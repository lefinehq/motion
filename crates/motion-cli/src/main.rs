//! The `motion` command-line tool, and the only place that prints: output to
//! stdout; failures, logs and timings to stderr. It exits 0, 1 when a check
//! has an error line, or a failure's [`Kind::exit_code`](motion_engine::Kind::exit_code).

mod commands;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::builder::PossibleValuesParser;
use clap::{Parser, Subcommand, ValueEnum};
use motion_encode::{Codec, VideoQuality};
use motion_engine::Quality;
use motion_engine::telemetry::timing::{REPORT_TARGET, Timings, millis};
use motion_engine::telemetry::{Format, LevelFilter, Options};
use strum::VariantNames;
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
    /// Draw frames on at most N threads; one per processor when not given.
    /// Each thread holds the frame it draws, so fewer take less memory.
    #[arg(long, value_name = "N", global = true)]
    threads: Option<std::num::NonZero<usize>>,
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
    /// Check a scene for problems such as layers leaving the frame, text too faint to read or flashing; exits 1 on errors.
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
        /// Layers to show, by address, such as `title` or `card/title`;
        /// every layer at any depth when none are given.
        ids: Vec<String>,
    },
    /// Make every variant of one master: apply each variant's edits through
    /// the locks, check it, and render those that pass.
    Batch {
        /// Path to the batch file: the master, the output folder and the variants.
        manifest: PathBuf,
        /// Apply and check every variant, and render none.
        #[arg(long)]
        check: bool,
        /// Print the outcomes as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run an agent on every brief of a set and record what each run cost
    /// and what `check` finds in the scene it wrote; scores nothing.
    Bench {
        /// The folder of briefs: Markdown files, in a folder per use case.
        briefs: PathBuf,
        /// The agent: a shell command, run in each brief's own folder with
        /// the brief on stdin and its path in `MOTION_BRIEF`. It writes
        /// `scene.json` there; its last line of output may be a JSON object
        /// with its `tokens` and `steps`, or `usage` and `num_turns`.
        #[arg(long)]
        agent: String,
        /// The folder for the runs: one folder per brief, and `results.jsonl`.
        #[arg(short, long)]
        out: PathBuf,
        /// Seconds an agent may take on one brief before it is stopped.
        #[arg(long, default_value_t = 1800)]
        timeout: u64,
    },
    /// List what a scene's layers and slots do, with what each role holds.
    Roles {
        /// Path to the scene file.
        scene: PathBuf,
        /// Print them as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List the slots of a scene and of its compositions, with how many
    /// values follow each.
    Slots {
        /// Path to the scene file.
        scene: PathBuf,
        /// Print them as JSON, with the place of each value that follows.
        #[arg(long)]
        json: bool,
    },
    /// Apply an edit line such as `card fill=#FF0000` and save the scene.
    Edit {
        /// Path to the scene file.
        scene: PathBuf,
        /// A layer id (or `scene`) followed by `key=value` pairs.
        line: String,
    },
    /// Draw frames of a scene into a video file, or into PNG files named
    /// `frame-00000.png`.
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
        /// A video file, whose extension picks the container (`.mp4`), or
        /// else the folder the PNG files are written to, created if missing.
        #[arg(long, value_name = "FILE|DIR")]
        out: PathBuf,
        /// How colours are computed: `precise` for delivered frames, `fast`
        /// for previews.
        #[arg(long, value_enum, default_value = "precise")]
        quality: QualityArg,
        /// The codec of a video file; the first this build has of H.264 and
        /// AV1 when not given.
        #[arg(long, value_parser = PossibleValuesParser::new(<Codec as VariantNames>::VARIANTS))]
        codec: Option<String>,
        /// What a video file is for: `draft` to check timing, `web` to
        /// deliver, `master` to edit or encode again.
        #[arg(
            long,
            value_parser = PossibleValuesParser::new(<VideoQuality as VariantNames>::VARIANTS)
        )]
        video_quality: Option<String>,
    },
    /// Compare a scene with a reference render's frames: the error where the
    /// scene draws, and over everything it draws in the compared frames.
    Diff {
        /// Path to the scene file.
        scene: PathBuf,
        /// The folder of reference frames: PNG files of the scene's size,
        /// in name order.
        #[arg(long, value_name = "DIR")]
        reference: PathBuf,
        /// The reference's frame rate, such as `59.94`.
        #[arg(long)]
        fps: f64,
        /// The scene time of the first reference frame, in seconds.
        #[arg(long, default_value_t = 0.0)]
        start: f64,
        /// How far a channel may differ, from 0 to 1, before its pixel fails.
        #[arg(long, default_value_t = motion_engine::DiffOptions::TOLERANCE)]
        tolerance: f64,
        /// Write every frame's numbers to this JSON file.
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
        /// Write one error image per frame into this folder.
        #[arg(long, value_name = "DIR")]
        images: Option<PathBuf>,
        /// A layer to leave out of the drawing and the measure, by address,
        /// such as a placeholder the reference fills (`plate`, or
        /// `card/photo` inside a composition); repeat for more.
        #[arg(long, value_name = "ADDRESS")]
        ignore: Vec<motion_engine::Address>,
    },
    /// Measure every sample of a local manifest against its reference, and
    /// compare the numbers with a ledger.
    Fidelity {
        /// The manifest: a JSON file listing the samples, with paths
        /// relative to it.
        manifest: PathBuf,
        /// The ledger to compare with.
        #[arg(long, value_name = "FILE")]
        ledger: Option<PathBuf>,
        /// Write the new numbers into the ledger.
        #[arg(long, requires = "ledger")]
        write: bool,
        /// Where each sample's scene and error images go.
        #[arg(long, value_name = "DIR", default_value = "target/fidelity")]
        work: PathBuf,
        /// Write one error image per frame.
        #[arg(long)]
        images: bool,
        /// Only this sample, by id; repeat for more.
        #[arg(long, value_name = "ID")]
        only: Vec<String>,
    },
    /// Import an `.aep` project or a `.mogrt` template as a scene, and print
    /// what the import mapped, defaulted, ignored or could not support yet.
    /// A template's controls become the scene's slots.
    Import {
        /// Path to the `.aep` or `.mogrt` file.
        project: PathBuf,
        /// The scene file to write.
        #[arg(long, short, value_name = "FILE")]
        out: PathBuf,
        /// The composition that becomes the scene, by name, or by item id
        /// (as `motion inspect` prints it) where names repeat; by default
        /// the one composition no other draws.
        #[arg(long, value_name = "NAME|ID")]
        comp: Option<String>,
        /// A font file or folder for the scene's `fonts`, relative to the
        /// scene file; repeat for more.
        #[arg(long = "font", value_name = "PATH")]
        fonts: Vec<String>,
        /// Find the fonts the project's text uses and the scene's `fonts`
        /// lack, pick each family's release by the glyphs and lines the
        /// project stored, and lock them beside the scene.
        #[arg(long)]
        fetch_fonts: bool,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Write an edited scene back into the `.aep` project it was imported
    /// from: each changed value the project stores is patched in, every
    /// other byte is kept, and what cannot go back is listed with why.
    Export {
        /// Path to the edited scene file.
        scene: PathBuf,
        /// The `.aep` project the scene was imported from.
        #[arg(long, value_name = "FILE")]
        project: PathBuf,
        /// Where to save the patched project; may be the project itself.
        #[arg(long, short, value_name = "FILE")]
        out: PathBuf,
        /// The composition the scene was imported from, when one was named.
        #[arg(long, value_name = "NAME")]
        comp: Option<String>,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Fonts: find the ones a scene uses and lock them.
    Fonts {
        #[command(subcommand)]
        command: FontsCommand,
    },
    /// Print an `.aep` project, or the one inside a `.mogrt` template: its
    /// items, layers and properties, then what the reader could not decode
    /// yet.
    Inspect {
        /// Path to the `.aep` or `.mogrt` file.
        project: PathBuf,
        /// Print the model and the report as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// The `motion fonts` commands.
#[derive(Subcommand)]
enum FontsCommand {
    /// Lock every font the scene's text uses and its `fonts` lack, from
    /// open-licensed sources, and download the locked files into the cache.
    /// A face already locked keeps its file.
    Fetch {
        /// Path to the scene file.
        scene: PathBuf,
        /// Resolve every face again, not only the new ones.
        #[arg(long)]
        update: bool,
        /// Only take releases whose tag or commit starts with this.
        #[arg(long, value_name = "REF")]
        release: Option<String>,
    },
}

impl Command {
    /// The file the command works on.
    fn file(&self) -> &Path {
        match self {
            Command::Check { scene }
            | Command::At { scene, .. }
            | Command::Slots { scene, .. }
            | Command::Roles { scene, .. }
            | Command::Edit { scene, .. }
            | Command::Render { scene, .. }
            | Command::Diff { scene, .. }
            | Command::Export { scene, .. } => scene,
            Command::Import { project, .. } | Command::Inspect { project, .. } => project,
            Command::Fidelity { manifest, .. } | Command::Batch { manifest, .. } => manifest,
            Command::Bench { briefs, .. } => briefs,
            Command::Fonts {
                command: FontsCommand::Fetch { scene, .. },
            } => scene,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Some(threads) = cli.threads {
        motion_engine::limit_threads(threads);
    }
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
        Command::Slots { scene, json } => commands::slots::run(&scene, json),
        Command::Roles { scene, json } => commands::roles::run(&scene, json),
        Command::Batch {
            manifest,
            check,
            json,
        } => commands::batch::run(&manifest, check, json),
        Command::Bench {
            briefs,
            agent,
            out,
            timeout,
        } => commands::bench::run(&briefs, &agent, &out, Duration::from_secs(timeout)),
        Command::At { scene, time, ids } => commands::at::run(&scene, &time, &ids),
        Command::Edit { scene, line } => commands::edit::run(&scene, &line),
        Command::Render {
            scene,
            frames,
            frame,
            out,
            quality,
            codec,
            video_quality,
        } => commands::render::Encoding::named(codec.as_deref(), video_quality.as_deref())
            .and_then(|encoding| {
                commands::render::run(
                    &scene,
                    frames.or(frame.map(commands::render::Frames::one)),
                    &out,
                    quality.into(),
                    encoding,
                )
            }),
        Command::Diff {
            scene,
            reference,
            fps,
            start,
            tolerance,
            json,
            images,
            ignore,
        } => commands::diff::run(
            &scene,
            &commands::diff::Args {
                reference: &reference,
                fps,
                start,
                tolerance,
                json: json.as_deref(),
                images: images.as_deref(),
                ignore: &ignore,
            },
        ),
        Command::Fidelity {
            manifest,
            ledger,
            write,
            work,
            images,
            only,
        } => commands::fidelity::run(&commands::fidelity::Args {
            manifest: &manifest,
            ledger: ledger.as_deref(),
            write,
            work: &work,
            images,
            only: &only,
        }),
        Command::Import {
            project,
            out,
            comp,
            fonts,
            fetch_fonts,
            json,
        } => commands::import::run(
            &project,
            &motion_engine::ImportOptions {
                comp,
                fonts,
                ..Default::default()
            },
            &out,
            &commands::import::Args { fetch_fonts, json },
        ),
        Command::Export {
            scene,
            project,
            out,
            comp,
            json,
        } => commands::export::run(commands::export::Args {
            scene: &scene,
            project: &project,
            out: &out,
            comp,
            json,
        }),
        Command::Fonts {
            command:
                FontsCommand::Fetch {
                    scene,
                    update,
                    release,
                },
        } => commands::fonts::fetch(&scene, update, release.as_deref()),
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
