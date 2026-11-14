//! Unit tests for `src/timing.rs`.

use tracing::{debug_span, field, info_span, trace_span};
use tracing_subscriber::layer::SubscriberExt;

use super::*;

/// Runs `body` with only the timings layer installed.
fn timed(body: impl FnOnce()) -> Timings {
    let timings = Timings::default();
    let subscriber = tracing_subscriber::registry().with(timings.layer());
    tracing::subscriber::with_default(subscriber, body);
    timings
}

fn names(timings: &Timings) -> Vec<String> {
    timings.steps().into_iter().map(|(name, _)| name).collect()
}

#[test]
fn records_nested_spans_in_first_seen_order() {
    let timings = timed(|| {
        let _outer = debug_span!("stage", stage = "edit").entered();
        let _inner = trace_span!("stage", stage = "check").entered();
    });
    let steps = timings.steps();
    assert_eq!(names(&timings), ["edit", "check"]);
    // The inner span closes inside the outer one, so it cannot be longer.
    assert!(steps[0].1 >= steps[1].1, "{steps:?}");
}

#[test]
fn spans_without_a_stage_field_are_not_timed() {
    let timings = timed(|| {
        let _span = info_span!("compile", nodes = 2).entered();
    });
    assert!(timings.steps().is_empty());
}

#[test]
fn a_stage_without_a_value_is_keyed_by_the_span_name() {
    let timings = timed(|| {
        let _span = debug_span!("render", stage = field::Empty).entered();
    });
    assert_eq!(names(&timings), ["render"]);
}

#[test]
fn repeated_steps_are_summed() {
    let timings = timed(|| {
        for _ in 0..3 {
            let _span = debug_span!("stage", stage = "evaluate").entered();
        }
    });
    assert_eq!(names(&timings), ["evaluate"]);
}

#[test]
fn report_lists_each_step_in_milliseconds() {
    let timings = Timings::default();
    let parse = timings.slot("parse".into());
    let compile = timings.slot("compile".into());
    timings.add(parse, Duration::from_micros(400));
    timings.add(compile, Duration::from_micros(1250));
    timings.add(parse, Duration::from_micros(100));
    assert_eq!(timings.report(), "timings parse=0.50ms compile=1.25ms");
}

#[test]
fn an_empty_report_is_just_the_label() {
    assert_eq!(Timings::default().report(), "timings");
}
