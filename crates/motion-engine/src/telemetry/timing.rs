//! Step timings: a layer that adds up how long each engine step took. It
//! knows a step by its span's target and name, so library spans never count.

use std::fmt::{self, Write as _};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// The target of the engine's step spans.
pub const STAGE_TARGET: &str = "motion_engine::stage";

/// The name of the engine's step spans.
pub const STAGE_SPAN: &str = "stage";

/// The field of a step span that holds the step's name.
pub const STAGE_FIELD: &str = "stage";

/// The target of the per-step timing events in JSON log mode.
pub const REPORT_TARGET: &str = "motion::timings";

/// The total time of each step, first seen first; a nested step also counts
/// in its parent's. Clones share the totals, so the caller keeps a handle.
#[derive(Debug, Clone, Default)]
pub struct Timings {
    steps: Arc<Mutex<Vec<(String, Duration)>>>,
}

impl Timings {
    /// A layer that records the engine's step spans, whatever their level.
    pub fn layer<S>(&self) -> impl Layer<S> + Send + Sync + use<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        Recorder {
            timings: self.clone(),
        }
        .with_filter(filter_fn(|meta| {
            meta.is_span() && meta.target() == STAGE_TARGET && meta.name() == STAGE_SPAN
        }))
    }

    /// Each step's name and total time so far, first seen first.
    pub fn steps(&self) -> Vec<(String, Duration)> {
        self.lock().clone()
    }

    /// The line the CLI prints: `timings parse=0.40ms compile=0.10ms`.
    pub fn report(&self) -> String {
        let mut line = String::from("timings");
        for (name, total) in self.lock().iter() {
            // Writing to a `String` cannot fail.
            let _ = write!(line, " {name}={:.2}ms", millis(*total));
        }
        line
    }

    /// The index of `name`'s entry, added at the end when it is new.
    fn slot(&self, name: String) -> usize {
        let mut steps = self.lock();
        match steps.iter().position(|(seen, _)| *seen == name) {
            Some(slot) => slot,
            None => {
                steps.push((name, Duration::ZERO));
                steps.len() - 1
            }
        }
    }

    fn add(&self, slot: usize, elapsed: Duration) {
        self.lock()[slot].1 += elapsed;
    }

    /// The totals. A poisoned lock is still usable: every update is a single
    /// addition.
    fn lock(&self) -> MutexGuard<'_, Vec<(String, Duration)>> {
        self.steps.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `total` in milliseconds, rounded to two decimals, as every report shows it.
pub fn millis(total: Duration) -> f64 {
    (total.as_secs_f64() * 100_000.0).round() / 100.0
}

/// The layer behind [`Timings::layer`].
struct Recorder {
    timings: Timings,
}

/// Stored in a span's extensions while it is open.
struct Started {
    slot: usize,
    at: Instant,
}

impl<S> Layer<S> for Recorder
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut stage = StageName(None);
        attrs.record(&mut stage);
        // A step span without a name has nothing to be keyed by.
        let Some(name) = stage.0 else {
            return;
        };
        let slot = self.timings.slot(name);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Started {
                slot,
                at: Instant::now(),
            });
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        if let Some(started) = span.extensions().get::<Started>() {
            self.timings.add(started.slot, started.at.elapsed());
        }
    }
}

/// Reads the [`STAGE_FIELD`] of a new span.
struct StageName(Option<String>);

impl Visit for StageName {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == STAGE_FIELD {
            self.0 = Some(value.to_owned());
        }
    }

    // Step names are strings, which arrive through `record_str`.
    fn record_debug(&mut self, _: &Field, _: &dyn fmt::Debug) {}
}

#[cfg(test)]
#[path = "../../tests/unit/telemetry/timing.rs"]
mod tests;
