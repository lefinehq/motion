//! Unit tests for `src/session.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tracing::Subscriber;
use tracing::span::{Attributes, Id};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};

use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::telemetry::timing::Timings;
use crate::test_support::with_subscriber;

const LAUNCH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/launch.json");

/// A copy of the launch example in a fresh directory of its own.
fn scratch_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-engine-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("scene.json");
    fs::copy(LAUNCH, &path).expect("copy scene");
    path
}

#[test]
fn the_launch_example_checks_ok() {
    let session = Session::open(LAUNCH).expect("open");
    assert_eq!(session.check().expect("check").to_string(), "OK 4");
    assert_eq!(session.path(), Some(Path::new(LAUNCH)));
}

#[test]
fn at_reads_seconds_with_the_scene_fps() {
    let session = Session::open(LAUNCH).expect("open");
    let ids = ["title".to_string()];
    let secs = session.at("1s", &ids).expect("at 1s");
    assert!(secs.starts_with("title x="), "{secs}");
    assert_eq!(session.at("30f", &ids), Ok(secs));
}

#[test]
fn at_rejects_a_bad_time_and_an_unknown_id() {
    let session = Session::open(LAUNCH).expect("open");
    assert_eq!(session.at("soon", &[]).unwrap_err().kind, Kind::Syntax);
    let unknown = session.at("1s", &["nope".to_string()]).unwrap_err();
    assert_eq!(unknown.kind, Kind::UnknownName);
    assert!(unknown.message.contains("`nope`"), "{unknown}");
}

#[test]
fn frame_evaluates_every_node() {
    let session = Session::open(LAUNCH).expect("open");
    let state = session.frame(30.0).expect("frame");
    assert_eq!(state.frame, 30.0);
    assert_eq!(state.nodes.len(), 2);
}

#[test]
fn a_missing_file_is_an_io_diagnostic() {
    let e = Session::open("/definitely/not/here.json").unwrap_err();
    assert_eq!(e.kind, Kind::Io);
    assert!(e.message.contains("/definitely/not/here.json"), "{e}");
}

#[test]
fn edit_then_save_writes_the_canonical_file() {
    let path = scratch_copy("edit_save");
    let mut session = Session::open(&path).expect("open");
    let report = session.edit("card at=\"center 2000\"").expect("edit");
    assert!(report.has_errors(), "{report}");
    session.save().expect("save");
    let text = fs::read_to_string(&path).expect("read back");
    assert_eq!(text, write_document(session.doc.raw()));
    let reopened = Session::open(&path).expect("reopen");
    assert_eq!(reopened.scene(), session.scene());
    assert!(!temp_path(&path).exists());
}

#[test]
fn a_failed_edit_leaves_the_session_and_file_as_they_were() {
    let path = scratch_copy("edit_failure");
    let before = fs::read(&path).expect("read");
    let mut session = Session::open(&path).expect("open");
    let scene = session.scene().clone();
    assert_eq!(
        session.edit("card fill=blue").unwrap_err().kind,
        Kind::Syntax
    );
    assert!(session.edit("card").is_err());
    assert_eq!(session.scene(), &scene);
    assert_eq!(fs::read(&path).expect("read"), before);
}

#[test]
fn an_edit_drops_the_compiled_graph() {
    let mut session = Session::open(LAUNCH).expect("open");
    let width = |s: &Session| s.graph().expect("graph").nodes[1].bounds.width();
    assert_eq!(width(&session), 300.0);
    // The edit's check compiles the new scene, so the graph is fresh.
    session.edit("card rect=500x400").expect("edit");
    assert_eq!(width(&session), 500.0);
}

#[test]
fn a_parsed_session_has_nowhere_to_save() {
    let text = fs::read_to_string(LAUNCH).expect("read");
    let session: Session = text.parse().expect("parse");
    assert_eq!(session.path(), None);
    assert_eq!(session.save().unwrap_err().kind, Kind::Missing);
}

#[rstest]
#[case("dir/scene.json", "dir/scene.json.tmp")]
#[case("scene.json", "scene.json.tmp")]
fn temp_path_is_a_sibling_with_a_suffix(#[case] path: &str, #[case] temp: &str) {
    assert_eq!(temp_path(Path::new(path)), Path::new(temp));
}

/// A layer that counts the `compile` spans of `motion-scene`, one per
/// compiled graph.
#[derive(Clone, Default)]
struct Compiles(Arc<AtomicUsize>);

impl Compiles {
    fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl<S: Subscriber> Layer<S> for Compiles {
    fn on_new_span(&self, attrs: &Attributes<'_>, _: &Id, _: Context<'_, S>) {
        let meta = attrs.metadata();
        if meta.target() == "motion_scene::compile" && meta.name() == "compile" {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[test]
fn queries_on_one_session_compile_once_until_an_edit() {
    let compiles = Compiles::default();
    let subscriber = tracing_subscriber::registry().with(compiles.clone());
    with_subscriber(subscriber, || {
        let mut session = Session::open(LAUNCH).expect("open");
        assert!(!session.check().expect("check").has_errors());
        session.at("1s", &[]).expect("at");
        session.frame(30.0).expect("frame");
        assert_eq!(compiles.count(), 1);
        // The edit's check compiles the new scene; later queries reuse it.
        session.edit("card rect=500x400").expect("edit");
        session.at("1s", &[]).expect("at");
        assert_eq!(compiles.count(), 2);
    });
}

#[test]
fn every_step_is_timed_under_its_own_name() {
    let timings = Timings::default();
    let subscriber = tracing_subscriber::registry().with(timings.layer());
    with_subscriber(subscriber, || {
        let session = Session::open(LAUNCH).expect("open");
        session.at("1s", &[]).expect("at");
        session.frame(30.0).expect("frame");
    });
    let names: Vec<String> = timings.steps().into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        [
            "read", "parse", "time", "fonts", "compile", "at", "evaluate"
        ]
    );
}

/// The raster crate's fixture font, an OFL subset whose PostScript name is
/// `FixtureSans-Black`.
const FIXTURE_FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../motion-raster/tests/fixtures/FixtureSans-Black.ttf"
);

/// A scene file in a fresh directory, with the fixture font copied into
/// `fonts/` beside it, holding `json`.
fn scene_with_fonts(name: &str, json: &str) -> PathBuf {
    let path = scratch_copy(name);
    let fonts = path.with_file_name("fonts");
    fs::create_dir_all(&fonts).expect("create fonts dir");
    fs::copy(FIXTURE_FONT, fonts.join("FixtureSans-Black.ttf")).expect("copy font");
    fs::write(&path, json).expect("write scene");
    path
}

#[test]
fn fonts_resolve_beside_the_scene_file_and_set_its_text() {
    let path = scene_with_fonts(
        "fonts",
        r#"{"motion":1,"dur":"1s","fonts":["fonts"],"layers":[{"id":"t","text":"AB","font":"FixtureSans-Black 40"}]}"#,
    );
    let session = Session::open(&path).expect("open");
    let fonts = session.fonts().expect("fonts");
    assert!(
        fonts
            .faces()
            .iter()
            .any(|f| f.postscript_name == "FixtureSans-Black")
    );
    let graph = session.graph().expect("graph");
    assert!(graph.nodes[0].bounds.width() > 0.0);
}

#[test]
fn a_missing_font_file_or_face_is_reported() {
    let missing = scene_with_fonts(
        "missing-font",
        r#"{"motion":1,"dur":"1s","fonts":["fonts","nowhere.ttf"],"layers":[]}"#,
    );
    let error = Session::open(&missing).expect("open").graph().unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::Io, Some("fonts[1]"))
    );
    let unknown = scene_with_fonts(
        "unknown-face",
        r#"{"motion":1,"dur":"1s","layers":[{"id":"t","text":"A","font":"Nope-Bold 10"}]}"#,
    );
    let error = Session::open(&unknown).expect("open").check().unwrap_err();
    assert_eq!(
        (error.kind, error.path.as_deref()),
        (Kind::UnknownName, Some("layers[0].font"))
    );
}

/// A 4x2 scene of 5 frames whose background is red.
const TINY: &str = r##"{"motion":1,"size":"4x2","fps":5,"dur":"1s","bg":"#FF0000","layers":[]}"##;

#[test]
fn a_frame_is_drawn_at_the_scene_size() {
    let session: Session = TINY.parse().unwrap();
    let image = session.render_with(0.0, RenderOptions::GOLDEN).unwrap();
    assert_eq!((image.width(), image.height()), (4, 2));
    assert_eq!(image.pixel(3, 1), Some([255, 0, 0, 255]));
    assert_eq!(session.render(0.0).unwrap(), image);
}

#[test]
fn the_frames_run_to_the_length_rounded_up() {
    let session: Session = TINY.parse().unwrap();
    assert_eq!(session.frames().unwrap(), 0..5);
    let session: Session = TINY.replace("1s", "1.1s").parse().unwrap();
    assert_eq!(session.frames().unwrap(), 0..6);
}

#[test]
fn every_frame_of_a_range_is_drawn_once() {
    let session: Session = TINY.parse().unwrap();
    let drawn = Mutex::new(Vec::new());
    session
        .render_frames(1..5, RenderOptions::GOLDEN, |frame, image| {
            assert_eq!(image.width(), 4);
            drawn.lock().unwrap().push(frame);
            Ok(())
        })
        .unwrap();
    let mut drawn = drawn.into_inner().unwrap();
    drawn.sort_unstable();
    assert_eq!(drawn, [1, 2, 3, 4]);
}

#[rstest]
#[case::past_the_end(3..6)]
#[case::backwards(Range { start: 3, end: 2 })]
fn frames_outside_the_scene_are_refused(#[case] frames: Range<u32>) {
    let session: Session = TINY.parse().unwrap();
    let error = session
        .render_frames(frames, RenderOptions::GOLDEN, |_, _| {
            panic!("nothing is drawn")
        })
        .unwrap_err();
    assert_eq!(error.kind, Kind::OutOfRange);
    assert_eq!(
        error.expected.as_deref(),
        Some("frames within the scene's 0..5 (an end past the last frame)")
    );
}

#[test]
fn the_first_failing_frame_is_reported_with_its_number() {
    let session: Session = TINY.parse().unwrap();
    let error = session
        .render_frames(0..5, RenderOptions::GOLDEN, |frame, _| match frame {
            2 | 4 => Err(Diagnostic::new(Kind::Io, "disk full")),
            _ => Ok(()),
        })
        .unwrap_err();
    assert_eq!(error.kind, Kind::Io);
    assert_eq!(error.message, "frame 2: disk full");
}

#[test]
fn render_is_timed_as_display_and_render_steps() {
    let timings = Timings::default();
    with_subscriber(tracing_subscriber::registry().with(timings.layer()), || {
        let session: Session = TINY.parse().unwrap();
        session.render(0.0).unwrap();
    });
    let steps: Vec<String> = timings.steps().into_iter().map(|(s, _)| s).collect();
    for step in ["evaluate", "display", "render"] {
        assert!(steps.iter().any(|s| s == step), "{step} in {steps:?}");
    }
}
