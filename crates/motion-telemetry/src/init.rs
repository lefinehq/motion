//! Installing the subscriber: filter, format and the optional timings.
//!
//! The filter comes from the [`ENV_VAR`] environment variable when it is set
//! (`EnvFilter` syntax, such as `motion_query=debug`), and from
//! [`Options::level`] otherwise. Everything is written to stderr.

use std::io::IsTerminal;

use tracing::Subscriber;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt};

use crate::timing::Timings;

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
    /// Warnings and errors as text, without timings.
    fn default() -> Self {
        Self {
            level: LevelFilter::WARN,
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
    let installed = subscriber(
        filter(options.level),
        options.format,
        std::io::stderr,
        std::io::stderr().is_terminal(),
        timings.clone(),
    )
    .try_init();
    match installed {
        Ok(()) => Guard { timings },
        Err(_) => Guard::default(),
    }
}

/// The filter from [`ENV_VAR`], or `level` when the variable is unset or
/// holds no valid directive.
fn filter(level: LevelFilter) -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(level.into())
        .with_env_var(ENV_VAR)
        .from_env_lossy()
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
#[path = "../tests/unit/init.rs"]
mod tests;
