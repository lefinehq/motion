//! Unit tests for `src/stage.rs`.

use std::fmt;
use std::sync::{Arc, Mutex};

use motion_diagnostic::Kind;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};

use super::*;

/// One captured event: its level and its fields, as `name=value` text.
#[derive(Debug)]
struct Captured {
    level: Level,
    fields: Vec<(String, String)>,
}

impl Captured {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A layer that keeps every event.
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<Captured>>>);

impl<S: Subscriber> Layer<S> for Events {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.0.lock().expect("events lock").push(Captured {
            level: *event.metadata().level(),
            fields: fields.0,
        });
    }
}

#[derive(Default)]
struct Fields(Vec<(String, String)>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name().to_owned(), value.to_owned()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
}

/// Runs `body` with a capturing subscriber and returns the events.
fn events_of<T>(body: impl FnOnce() -> T) -> (T, Vec<Captured>) {
    let events = Events::default();
    let subscriber = tracing_subscriber::registry().with(events.clone());
    let out = tracing::subscriber::with_default(subscriber, body);
    let captured = std::mem::take(&mut *events.0.lock().expect("events lock"));
    (out, captured)
}

#[test]
fn a_failing_step_logs_one_error_with_kind_and_path() {
    let failure = Diagnostic::new(Kind::OutOfRange, "too far").at("layers[0].at");
    let (result, events) = events_of(|| stage("check", || Err::<(), _>(failure.clone())));
    assert_eq!(result, Err(failure));
    assert_eq!(events.len(), 1, "{events:?}");
    let event = &events[0];
    assert_eq!(event.level, Level::ERROR);
    assert_eq!(event.field("kind"), Some("OutOfRange"));
    assert_eq!(event.field("path"), Some("layers[0].at"));
    assert_eq!(event.field("message"), Some("too far"));
}

#[test]
fn a_failure_without_a_path_logs_no_path_field() {
    let (_, events) = events_of(|| stage("time", || Err::<(), _>(Diagnostic::syntax("bad"))));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].field("path"), None);
}

#[test]
fn a_successful_step_logs_nothing() {
    let (result, events) = events_of(|| stage("parse", || Ok(7)));
    assert_eq!(result, Ok(7));
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn an_infallible_step_returns_its_value() {
    let (value, events) = events_of(|| stage_ok("compile", || "graph"));
    assert_eq!(value, "graph");
    assert!(events.is_empty());
}
