//! Step timings: a layer that adds up how long each engine step took.
//!
//! The engine runs every step in a span whose [`STAGE_FIELD`] holds the
//! step's name (`parse`, `compile`, `check`). [`Timings::layer`] times only
//! the spans that have that field, whatever their level and whatever the
//! log filter, from creation to close, keyed by the field's value. The
//! library crates' own spans have no such field, so they never add to the
//! line. A nested step counts in its own entry and in its parent's.

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

/// The span field that names a step.
pub const STAGE_FIELD: &str = "stage";

/// The total time of each step, in the order the steps were first seen.
///
/// A cheap handle: clones share the same totals, so the layer can record
/// while the caller keeps a handle for the [`report`](Timings::report).
#[derive(Debug, Clone, Default)]
pub struct Timings {
    steps: Arc<Mutex<Vec<(String, Duration)>>>,
}

impl Timings {
    /// A layer that records into these timings; it sees only the spans with
    /// a [`STAGE_FIELD`].
    pub fn layer<S>(&self) -> impl Layer<S> + Send + Sync + use<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        Recorder {
            timings: self.clone(),
        }
        .with_filter(filter_fn(|meta| {
            meta.is_span() && meta.fields().field(STAGE_FIELD).is_some()
        }))
    }

    /// Each step's name and total time so far, first seen first.
    pub fn steps(&self) -> Vec<(String, Duration)> {
        self.lock().clone()
    }

    /// The line the CLI prints: `timings parse=0.40ms compile=0.10ms`, in
    /// milliseconds with two decimals.
    pub fn report(&self) -> String {
        let mut line = String::from("timings");
        for (name, total) in self.lock().iter() {
            // Writing to a `String` cannot fail.
            let _ = write!(line, " {name}={:.2}ms", total.as_secs_f64() * 1000.0);
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

    /// The totals; a panic elsewhere while holding the lock leaves them
    /// usable, since every update is a single addition.
    fn lock(&self) -> MutexGuard<'_, Vec<(String, Duration)>> {
        self.steps.lock().unwrap_or_else(PoisonError::into_inner)
    }
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
        // A stage declared without a value falls back to the span's name.
        let name = stage
            .0
            .unwrap_or_else(|| attrs.metadata().name().to_owned());
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

    fn record_debug(&mut self, _: &Field, _: &dyn fmt::Debug) {
        // Step names are strings, which arrive through `record_str`.
    }
}

#[cfg(test)]
#[path = "../tests/unit/timing.rs"]
mod tests;
