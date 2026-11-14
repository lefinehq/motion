//! Unit tests for `src/init.rs`.

use std::io;
use std::sync::{Arc, Mutex};

use tracing::{debug, debug_span, warn};

use super::*;

/// A writer that keeps every byte, for reading the lines back.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("capture lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Capture {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().expect("capture lock").clone()).expect("utf-8")
    }
}

/// Runs `body` under the subscriber `init` would install, writing to a
/// capture, and returns what was written.
fn captured(directives: &str, format: Format, body: impl FnOnce()) -> String {
    let capture = Capture::default();
    let writer = capture.clone();
    let subscriber = subscriber(
        EnvFilter::new(directives),
        format,
        move || writer.clone(),
        false,
        None,
    );
    tracing::subscriber::with_default(subscriber, body);
    capture.text()
}

#[test]
fn init_twice_keeps_the_first_subscriber() {
    let options = Options {
        timings: true,
        ..Options::default()
    };
    let first = init(options);
    let second = init(options);
    assert!(first.timings().is_some());
    assert!(second.timings().is_none());
}

#[test]
fn json_format_writes_one_object_per_line() {
    let text = captured("debug", Format::Json, || {
        let _step = debug_span!("stage", stage = "parse").entered();
        debug!(kind = "Syntax", path = "layers[0].in", "rule rejected");
        warn!("suspicious");
    });
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert_eq!(lines[0]["level"], "DEBUG");
    assert_eq!(lines[0]["fields"]["message"], "rule rejected");
    assert_eq!(lines[0]["fields"]["path"], "layers[0].in");
    assert_eq!(lines[0]["span"]["stage"], "parse");
    assert_eq!(lines[1]["level"], "WARN");
    // The closed span gets a line of its own, with its time.
    assert_eq!(lines[2]["fields"]["message"], "close");
    assert_eq!(lines[2]["span"]["stage"], "parse");
}

#[test]
fn spans_below_the_level_write_no_close_line() {
    let text = captured("info", Format::Text, || {
        let _step = debug_span!("stage", stage = "parse").entered();
    });
    assert!(text.is_empty(), "{text}");
}

#[test]
fn the_filter_drops_quieter_events() {
    let text = captured("warn", Format::Text, || {
        debug!("hidden");
        warn!("shown");
    });
    assert!(text.contains("shown") && !text.contains("hidden"), "{text}");
}

#[test]
fn the_filter_takes_per_crate_directives() {
    let text = captured("warn,motion_telemetry=debug", Format::Text, || {
        debug!("shown");
    });
    assert!(text.contains("shown"), "{text}");
}

#[test]
fn timings_see_steps_the_filter_hides() {
    let timings = Timings::default();
    let subscriber = subscriber(
        EnvFilter::new("error"),
        Format::Text,
        io::sink,
        false,
        Some(timings.clone()),
    );
    tracing::subscriber::with_default(subscriber, || {
        let _step = debug_span!("stage", stage = "check").entered();
    });
    let names: Vec<String> = timings.steps().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["check"]);
}
