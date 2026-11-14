//! Installing the subscriber: filter, format and the optional timings.
//!
//! The filter comes from the [`ENV_VAR`] environment variable when it is set
//! (`EnvFilter` syntax, such as `motion_query=debug`), and from
//! [`Options::level`] otherwise. A value that is not a valid filter is
//! ignored with one `warn` event, written like any other log line.
//! Everything is written to stderr.

use std::env::{self, VarError};
use std::io::IsTerminal;

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
/// `EnvFilter` syntax: `debug`, or `warn,motion_query=debug`.
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
    /// Whether to add up step timings; read them from [`Guard::timings`].
    pub timings: bool,
}

impl Default for Options {
    /// No log lines, as text when turned on, without timings.
    fn default() -> Self {
        Self {
            level: LevelFilter::OFF,
            format: Format::Text,
            timings: false,
        }
    }
}

/// What [`init`] installed: today, the step timings when asked for.
///
/// Keep it until the end of `main`, then print its
/// [`report`](Timings::report) if there is one.
#[derive(Debug, Default)]
#[must_use = "the guard holds the step timings"]
pub struct Guard {
    timings: Option<Timings>,
}

impl Guard {
    /// The step timings, when [`Options::timings`] was on and this call
    /// installed the subscriber.
    pub fn timings(&self) -> Option<&Timings> {
        self.timings.as_ref()
    }
}

/// Installs the global subscriber, writing to stderr.
///
/// Safe to call more than once: when a subscriber is already installed (by
/// an earlier call or by a test harness), it is kept and the returned guard
/// has no timings.
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
        std::io::stderr,
        std::io::stderr().is_terminal(),
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

/// A value of [`ENV_VAR`] that is not a valid filter, kept until the
/// subscriber that logs it is installed.
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

/// The filter from `var`, the value of [`ENV_VAR`], or just `level` when it
/// is unset; or `level` and the rejection when it is not a valid filter.
///
/// The fallback lets this module's warnings through, so the rejection is
/// shown even when `level` hides warnings.
pub(crate) fn filter(
    level: LevelFilter,
    var: Result<String, VarError>,
) -> (EnvFilter, Option<Rejected>) {
    let builder = EnvFilter::builder().with_default_directive(level.into());
    let rejected = match var {
        Err(VarError::NotPresent) => None,
        Ok(value) => match builder.parse(&value) {
            Ok(filter) => return (filter, None),
            Err(e) => Some(Rejected {
                value,
                reason: e.to_string(),
            }),
        },
        Err(VarError::NotUnicode(value)) => Some(Rejected {
            value: value.to_string_lossy().into_owned(),
            reason: "not valid UTF-8".to_owned(),
        }),
    };
    let fallback = EnvFilter::default().add_directive(level.into());
    match rejected {
        None => (fallback, None),
        Some(rejected) => (
            fallback.add_directive(directive(module_path!(), LevelFilter::WARN)),
            Some(rejected),
        ),
    }
}

/// The directive `{target}={level}`.
fn directive(target: &str, level: LevelFilter) -> Directive {
    format!("{target}={level}")
        .parse()
        .expect("a module path and a level make a valid directive")
}

/// The subscriber [`init`] installs, with the writer left open so tests can
/// capture the lines. `ansi` colours text lines; it is on for terminals only.
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
    // A line per closed span shows the counts the spans carry (layers,
    // nodes, violations) and how long each took, at the span's own level.
    let lines = fmt::layer()
        .with_writer(writer)
        .with_ansi(ansi)
        .with_span_events(FmtSpan::CLOSE);
    let lines = match format {
        Format::Text => lines.boxed(),
        Format::Json => lines.json().boxed(),
    };
    // Each layer has its own filter: the level decides which lines are
    // written, but the timings still see every step.
    tracing_subscriber::registry()
        .with(lines.with_filter(filter))
        .with(timings.map(|timings| timings.layer()))
}

#[cfg(test)]
#[path = "../../tests/unit/telemetry/init.rs"]
mod tests;
