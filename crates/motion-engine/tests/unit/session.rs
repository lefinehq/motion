//! Unit tests for `src/session.rs`.

use super::*;

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
    assert_eq!(session.check().to_string(), "OK 4");
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
    assert_eq!(
        unknown.to_string(),
        "unknown layer id `nope`; expected one of title, card"
    );
}

#[test]
fn frame_evaluates_every_node() {
    let session = Session::open(LAUNCH).expect("open");
    let state = session.frame(30.0);
    assert_eq!(state.frame, 30.0);
    assert_eq!(state.nodes.len(), 2);
}

#[test]
fn a_missing_file_is_an_io_diagnostic() {
    let e = Session::open("/definitely/not/here.json").unwrap_err();
    assert_eq!(e.kind, Kind::Io);
    assert!(
        e.message
            .starts_with("cannot read `/definitely/not/here.json`: "),
        "{e}"
    );
}

#[test]
fn edit_then_save_writes_the_canonical_file() {
    let path = scratch_copy("edit_save");
    let mut session = Session::open(&path).expect("open");
    let report = session.edit("card at=\"center 2000\"").expect("edit");
    assert!(report.has_errors(), "{report}");
    session.save().expect("save");
    let text = fs::read_to_string(&path).expect("read back");
    assert_eq!(text, write_document(&session.doc.raw));
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
    let width = |s: &Session| s.graph().nodes[1].width;
    assert_eq!(width(&session), 300.0);
    // The edit's check compiles the new scene, so the graph is fresh.
    session.edit("card rect=500x400").expect("edit");
    assert_eq!(width(&session), 500.0);
}

#[test]
fn a_session_from_text_has_nowhere_to_save() {
    let text = fs::read_to_string(LAUNCH).expect("read");
    let session = Session::from_text(&text).expect("parse");
    assert_eq!(session.path(), None);
    assert_eq!(session.save().unwrap_err().kind, Kind::Missing);
}

#[test]
fn temp_path_is_a_sibling_with_a_suffix() {
    assert_eq!(
        temp_path(Path::new("dir/scene.json")),
        Path::new("dir/scene.json.tmp")
    );
    assert_eq!(
        temp_path(Path::new("scene.json")),
        Path::new("scene.json.tmp")
    );
}
