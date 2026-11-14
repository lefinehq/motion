//! [`init`]: the subscriber, filtered by [`ENV_VAR`] or [`Options::level`],
//! that writes every line to stderr.

use std::env::{self, VarError};
use std::io::{self, IsTerminal};

use tracing::{Subscriber, warn};
use tracing_subscriber::filter::{Directive, EnvFilter, LevelFilter};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt};

use super::timing::{REPORT_TARGET, Timings};

/// The environment variable that overrides [`Options::level`], in
/// `EnvFilter` syntax such as `warn,motion_query=debug`.
pub const ENV_VAR: &str = "MOTION_LOG";

/// How log lines are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// One human-readable line per event.
    #[default]
    Text,
    /// One JSON object per line, for machines and agents.
    Json,
}

/// What [`init`] installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The most verbose level shown when [`ENV_VAR`] is not set.
    pub level: LevelFilter,
    /// How each line is written.
    pub format: Format,
    /// Whether to add up step timings, read from [`Guard::timings`].
    pub timings: bool,
}

impl Default for Options {
    /// Logs off, so a plain failing run prints only its `error:` line.
    fn default() -> Self {
        Self {
            level: LevelFilter::OFF,
            format: Format::Text,
            timings: false,
        }
    }
}

/// What [`init`] installed; keep it until the end of `main`.
#[derive(Debug, Default)]
#[must_use = "the guard holds the step timings"]
pub struct Guard {
    timings: Option<Timings>,
}

impl Guard {
    /// The step timings, if asked for and this call installed the subscriber.
    pub fn timings(&self) -> Option<&Timings> {
        self.timings.as_ref()
    }
}

/// Installs the global subscriber. A subscriber installed earlier (by another
/// call or a test harness) is kept, and the guard then has no timings.
pub fn init(options: Options) -> Guard {
    let timings = options.timings.then(Timings::default);
    let (mut filter, rejected) = filter(options.level, env::var(ENV_VAR));
    if options.timings {
        // Asking for timings is asking for their events, whatever the level.
        filter = filter.add_directive(directive(REPORT_TARGET, LevelFilter::INFO));
    }
    let installed = subscriber(
        filter,
        options.format,
        io::stderr,
        io::stderr().is_terminal(),
        timings.clone(),
    )
    .try_init();
    match installed {
        Ok(()) => {
            if let Some(rejected) = rejected {
                rejected.log(options.level);
            }
            Guard { timings }
        }
        Err(_) => Guard::default(),
    }
}

/// An invalid [`ENV_VAR`] value, kept until there is a subscriber to log it.
#[derive(Debug)]
pub(crate) struct Rejected {
    /// The value as set, lossily decoded when it is not UTF-8.
    pub(crate) value: String,
    /// Why it was rejected.
    pub(crate) reason: String,
}

impl Rejected {
    /// The one `warn` event naming the ignored value.
    pub(crate) fn log(&self, level: LevelFilter) {
        warn!(
            value = %self.value,
            reason = %self.reason,
            "ignoring {ENV_VAR}, which is not a valid filter; using the level `{level}`"
        );
    }
}

/// The filter from `var` (the [`ENV_VAR`] value), or from `level`. After a
/// rejection it lets this module's warning through, whatever the level.
pub(crate) fn filter(
    level: LevelFilter,
    var: Result<String, VarError>,
) -> (EnvFilter, Option<Rejected>) {
    let fallback = || EnvFilter::default().add_directive(level.into());
    let rejected = match var {
        Err(VarError::NotPresent) => return (fallback(), None),
        Ok(value) => match EnvFilter::builder()
            .with_default_directive(level.into())
            .parse(&value)
        {
            Ok(filter) => return (filter, None),
            Err(e) => Rejected {
                value,
                reason: e.to_string(),
            },
        },
        Err(VarError::NotUnicode(value)) => Rejected {
            value: value.to_string_lossy().into_owned(),
            reason: "not valid UTF-8".to_owned(),
        },
    };
    let filter = fallback().add_directive(directive(module_path!(), LevelFilter::WARN));
    (filter, Some(rejected))
}

/// The directive `{target}={level}`.
fn directive(target: &str, level: LevelFilter) -> Directive {
    format!("{target}={level}")
        .parse()
        .expect("a module path and a level make a valid directive")
}

/// The subscriber [`init`] installs, with a writer that tests can capture.
pub(crate) fn subscriber<W>(
    filter: EnvFilter,
    format: Format,
    writer: W,
    ansi: bool,
    timings: Option<Timings>,
) -> impl Subscriber + for<'a> LookupSpan<'a> + Send + Sync + 'static
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    // A closed span's line shows its counts and how long it took.
    let lines = fmt::layer()
        .with_writer(writer)
        .with_ansi(ansi)
        .with_span_events(FmtSpan::CLOSE);
    let lines = match format {
        Format::Text => lines.boxed(),
        Format::Json => lines.json().boxed(),
    };
    // The filter is the lines' own, so the timings still see every step.
    tracing_subscriber::registry()
        .with(lines.with_filter(filter))
        .with(timings.map(|timings| timings.layer()))
}

#[cfg(test)]
#[path = "../../tests/unit/telemetry/init.rs"]
mod tests;
