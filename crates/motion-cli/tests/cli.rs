//! End-to-end tests: run the real `motion` binary and check its output and exit code.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const LAUNCH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/launch.json");
const OFFSCREEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/offscreen.json");

/// Runs `motion` with the given arguments.
fn motion(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_motion"))
        .args(args)
        .output()
        .expect("run motion")
}

/// A fresh scratch directory for one test (tests run in parallel).
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Copies `src` into a scratch directory and returns the copy's path.
fn copy_to(name: &str, src: &str) -> String {
    let path = scratch(name).join("scene.json");
    fs::copy(src, &path).expect("copy scene");
    path.to_str().expect("utf-8 path").to_string()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn check_clean_example_exits_zero() {
    let out = motion(&["check", LAUNCH]);
    assert_eq!(stdout(&out), "OK 4\n");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn check_offscreen_exits_one() {
    let out = motion(&["check", OFFSCREEN]);
    assert!(stdout(&out).starts_with("E offscreen card 0-149f +120px"));
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn at_accepts_seconds_and_frames() {
    let secs = motion(&["at", LAUNCH, "1s", "title"]);
    let frames = motion(&["at", LAUNCH, "30f", "title"]);
    assert_eq!(secs.status.code(), Some(0));
    assert!(!stdout(&secs).is_empty());
    assert_eq!(stdout(&secs), stdout(&frames));
}

#[test]
fn at_without_ids_prints_every_node() {
    let out = motion(&["at", LAUNCH, "1s"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.contains("title") && text.contains("card"), "{text}");
}

#[test]
fn at_unknown_id_exits_two() {
    let out = motion(&["at", LAUNCH, "1s", "nope"]);
    assert_eq!(
        stderr(&out),
        "error: unknown layer id `nope`; expected one of title, card\n"
    );
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn at_bad_time_exits_two() {
    let out = motion(&["at", LAUNCH, "soon", "title"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).starts_with("error: "));
}

#[test]
fn missing_file_exits_two() {
    let out = motion(&["check", "/definitely/not/here.json"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).starts_with("error: cannot read"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn edit_writes_canonical_file_and_prints_report() {
    let path = copy_to("edit_canonical", OFFSCREEN);
    let out = motion(&["edit", &path, "card at=\"center 700\""]);
    assert_eq!(stdout(&out), "OK 2\n");
    assert_eq!(out.status.code(), Some(0));
    let text = fs::read_to_string(&path).expect("read back");
    assert!(motion_format::parse_document(&text).is_ok());
    assert_eq!(text.lines().nth(1), Some("\"layers\":["));
}

#[test]
fn edit_takes_nested_keys_and_json_values() {
    let path = copy_to(
        "edit_nested_json",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/launch.json"),
    );
    let out = motion(&[
        "edit",
        &path,
        r#"title in=none anim.opacity=[["0f",0],["12f",1]]"#,
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = fs::read_to_string(&path).expect("read back");
    assert!(motion_format::parse_document(&text).is_ok());
    // The nested keys really landed in the file.
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    let anim = &doc["layers"][0]["anim"]["opacity"];
    assert_eq!(anim, &serde_json::json!([["0f", 0], ["12f", 1]]), "{text}");
    assert!(doc["layers"][0].get("in").is_none(), "{text}");
}

#[test]
fn edit_invalid_value_leaves_file_untouched() {
    let path = copy_to("edit_invalid", OFFSCREEN);
    let before = fs::read(&path).expect("read");
    let out = motion(&["edit", &path, "card fill=blue"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read(&path).expect("read"), before);
}

#[test]
fn edit_malformed_line_leaves_file_untouched() {
    let path = copy_to("edit_malformed", OFFSCREEN);
    let before = fs::read(&path).expect("read");
    let out = motion(&["edit", &path, "card"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).starts_with("error: "));
    assert_eq!(fs::read(&path).expect("read"), before);
}

#[test]
fn edit_that_leaves_a_violation_saves_and_exits_one() {
    let path = copy_to("edit_violation", OFFSCREEN);
    let before = fs::read(&path).expect("read");
    let out = motion(&["edit", &path, "card at=\"center 1100\""]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).starts_with("E offscreen card"),
        "{}",
        stdout(&out)
    );
    assert_ne!(fs::read(&path).expect("read"), before);
}

#[test]
fn invalid_scene_exits_two_with_path() {
    let text = fs::read_to_string(LAUNCH).expect("read launch");
    let bad = text.replace("\"in\":\"fade-up 12f\"", "\"in\":\"zoom\"");
    let path = scratch("invalid_scene").join("bad.json");
    fs::write(&path, bad).expect("write");
    let out = motion(&["check", path.to_str().expect("utf-8")]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("layers[0].in: unknown preset `zoom`"),
        "{}",
        stderr(&out)
    );
}
