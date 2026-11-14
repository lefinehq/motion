//! End-to-end tests: run the real `motion` binary and check its output and
//! exit code. An error line is compared with the engine's own diagnostic, so
//! only the `error: …` format is pinned here, not the wording.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use motion_engine::{Diagnostic, Session};
use pretty_assertions::assert_eq;
use rstest::rstest;

const LAUNCH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/launch.json");
const OFFSCREEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/offscreen.json");

/// Runs `motion` with the given arguments, without a `MOTION_LOG` from the
/// environment.
fn motion(args: &[&str]) -> Output {
    run(
        Command::new(env!("CARGO_BIN_EXE_motion")).env_remove("MOTION_LOG"),
        args,
    )
}

/// Runs `motion` with `MOTION_LOG` set to `filter`.
fn motion_with_log(filter: &str, args: &[&str]) -> Output {
    run(
        Command::new(env!("CARGO_BIN_EXE_motion")).env("MOTION_LOG", filter),
        args,
    )
}

fn run(command: &mut Command, args: &[&str]) -> Output {
    command.args(args).output().expect("run motion")
}

/// The line `motion` prints to stderr for `diagnostic`.
fn error_line(diagnostic: &Diagnostic) -> String {
    format!("error: {diagnostic}\n")
}

/// What the engine says when asked for `ids` at `time` in the launch example.
fn at_error(time: &str, ids: &[&str]) -> Diagnostic {
    let ids: Vec<String> = ids.iter().map(|&id| id.to_owned()).collect();
    Session::open(LAUNCH)
        .expect("open")
        .at(time, &ids)
        .expect_err("a failing query")
}

/// What the engine says when opening `path`.
fn open_error(path: impl AsRef<Path>) -> Diagnostic {
    Session::open(path.as_ref()).expect_err("a failing open")
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
fn help_describes_the_tool_and_lists_its_commands() {
    let out = motion(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(
        text.starts_with(
            "Check, query, edit and render motion scenes, and inspect `.aep` projects\n"
        ),
        "{text}"
    );
    for command in ["check", "at", "edit", "render", "inspect"] {
        assert!(
            text.lines().any(|l| l.trim_start().starts_with(command)),
            "{command}: {text}"
        );
    }
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

#[rstest]
#[case::unknown_id("1s", "nope")]
#[case::bad_time("soon", "title")]
fn a_failing_at_exits_two(#[case] time: &str, #[case] id: &str) {
    let out = motion(&["at", LAUNCH, time, id]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stderr(&out), error_line(&at_error(time, &[id])));
}

#[test]
fn missing_file_exits_two() {
    let missing = "/definitely/not/here.json";
    let out = motion(&["check", missing]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stderr(&out), error_line(&open_error(missing)));
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
    let path = copy_to("edit_nested_json", LAUNCH);
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
    let malformed = Session::open(&path)
        .expect("open")
        .edit("card")
        .expect_err("a malformed line");
    assert_eq!(stderr(&out), error_line(&malformed));
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
    let invalid = open_error(&path);
    assert_eq!(invalid.path.as_deref(), Some("layers[0].in"));
    assert_eq!(stderr(&out), error_line(&invalid));
    assert!(stderr(&out).starts_with("error: layers[0].in: "));
}

#[test]
fn a_failure_is_logged_once_before_the_error_line() {
    let out = motion(&["--log", "error", "at", LAUNCH, "1s", "nope"]);
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(
        lines[0].contains("ERROR") && lines[0].contains("kind=UnknownName"),
        "{text}"
    );
    assert!(lines[1].starts_with("error: "), "{text}");
}

#[rstest]
#[case::unknown_id(&["at", LAUNCH, "1s", "nope"])]
#[case::bad_time(&["at", LAUNCH, "soon"])]
#[case::missing_file(&["check", "/definitely/not/here.json"])]
fn a_plain_failing_run_prints_exactly_one_error_line(#[case] args: &[&str]) {
    let out = motion(args);
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(lines[0].starts_with("error: "), "{text}");
}

#[test]
fn info_logs_one_line_per_command() {
    let out = motion(&["--log", "info", "check", LAUNCH]);
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(
        lines[0].contains(r#"command="check""#) && lines[0].contains(&format!("file={LAUNCH}")),
        "{text}"
    );
}

#[test]
fn an_invalid_motion_log_is_one_warning_then_the_level() {
    let out = motion_with_log("motion=loud", &["at", LAUNCH, "1s", "nope"]);
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(
        lines[0].contains(" WARN motion_engine::telemetry")
            && lines[0].contains("value=motion=loud"),
        "{text}"
    );
    assert_eq!(
        format!("{}\n", lines[1]),
        error_line(&at_error("1s", &["nope"]))
    );
}

#[rstest]
#[case::clean_check(&["check", LAUNCH])]
#[case::failing_check(&["check", OFFSCREEN])]
#[case::at(&["at", LAUNCH, "1s"])]
#[case::failing_at(&["at", LAUNCH, "1s", "nope"])]
fn json_logs_leave_stdout_unchanged(#[case] args: &[&str]) {
    let plain = motion(args);
    let logged = motion(&[&["--log", "debug", "--log-format", "json"], args].concat());
    assert_eq!(logged.stdout, plain.stdout);
    assert_eq!(logged.status.code(), plain.status.code());
    let text = stderr(&logged);
    let logs: Vec<&str> = text.lines().filter(|l| !l.starts_with("error: ")).collect();
    assert!(!logs.is_empty(), "{text}");
    for line in logs {
        let event: serde_json::Value = serde_json::from_str(line).expect(line);
        assert!(event["level"].is_string(), "{line}");
    }
}

#[test]
fn json_logs_carry_the_step_counts() {
    let out = motion(&["--log", "debug", "--log-format", "json", "check", LAUNCH]);
    let text = stderr(&out);
    assert!(
        text.lines()
            .any(|l| l.contains(r#""name":"check""#) && l.contains(r#""passed":4"#)),
        "{text}"
    );
}

#[test]
fn timings_print_one_line_on_stderr() {
    let out = motion(&["--timings", "check", LAUNCH]);
    assert_eq!(stdout(&out), "OK 4\n");
    let text = stderr(&out);
    let line = text
        .lines()
        .find(|l| l.starts_with("timings "))
        .unwrap_or_else(|| panic!("no timings line: {text}"));
    for step in ["read=", "parse=", "compile=", "check="] {
        assert!(line.contains(step), "{line}");
    }
}

#[test]
fn every_text_line_step_has_one_json_event() {
    let path = copy_to("json_timings", LAUNCH);
    for args in [
        &["check", LAUNCH][..],
        &["at", LAUNCH, "1s"],
        &["edit", &path, "card rect=500x400"],
    ] {
        let text = stderr(&motion(&[&["--timings"], args].concat()));
        let steps: Vec<&str> = text
            .strip_prefix("timings ")
            .unwrap_or_else(|| panic!("{args:?}: {text}"))
            .split_whitespace()
            .map(|step| step.split_once('=').expect("name=time").0)
            .collect();
        let json = stderr(&motion(
            &[&["--timings", "--log-format", "json"], args].concat(),
        ));
        let events: Vec<String> = json
            .lines()
            .map(|line| {
                let event: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
                assert_eq!(event["level"], "INFO", "{json}");
                assert_eq!(event["target"], "motion::timings", "{json}");
                assert!(event["fields"]["ms"].is_number(), "{json}");
                event["fields"]["stage"]
                    .as_str()
                    .unwrap_or_else(|| panic!("no stage: {json}"))
                    .to_owned()
            })
            .collect();
        assert_eq!(events, steps, "{args:?}");
    }
}

#[test]
fn timings_follow_a_failure_too() {
    let out = motion(&["at", LAUNCH, "soon", "--timings"]);
    assert_eq!(out.status.code(), Some(2));
    let text = stderr(&out);
    assert!(
        text.lines()
            .last()
            .is_some_and(|l| l.starts_with("timings read=")),
        "{text}"
    );
}

/// The smallest `.aep` project, a file version and an empty root folder,
/// written to a scratch directory.
fn empty_project(name: &str) -> String {
    let chunk = |tag: &[u8; 4], body: &[u8]| {
        let size = u32::try_from(body.len()).expect("a small body");
        [tag.as_slice(), &size.to_be_bytes(), body].concat()
    };
    let head = chunk(
        b"head",
        &[0, 94, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let file = chunk(
        b"RIFX",
        &[b"Egg!".as_slice(), &head, &chunk(b"LIST", b"Fold")].concat(),
    );
    let path = scratch(name).join("project.aep");
    fs::write(&path, file).expect("write project");
    path.to_str().expect("utf-8 path").to_string()
}

#[test]
fn inspect_prints_the_tree_then_the_report() {
    let path = empty_project("inspect-tree");
    let out = motion(&["inspect", &path]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(
        text.starts_with("project 94.9, unknown bits per channel\nreport\n"),
        "{text}"
    );
    assert!(
        text.ends_with("  opaque: none\n  skipped: none\n"),
        "{text}"
    );
}

#[test]
fn inspect_json_prints_the_model_and_the_report() {
    let path = empty_project("inspect-json");
    let out = motion(&["inspect", &path, "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(json["project"]["version"]["major"], 94);
    assert_eq!(json["report"]["layers"], serde_json::json!({}));
}

#[test]
fn inspect_a_file_that_is_not_a_project_exits_two() {
    let out = motion(&["inspect", LAUNCH]);
    assert_eq!(out.status.code(), Some(2));
    let error = motion_engine::import(LAUNCH).expect_err("not a project");
    assert_eq!(stderr(&out), error_line(&error));
}

#[test]
fn render_writes_one_numbered_png_per_frame() {
    let out = scratch("render-frames");
    let dir = out.to_str().expect("utf-8 path");
    let run = motion(&["render", LAUNCH, "--frames", "3-5", "--out", dir]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run), format!("3 frames written to {dir}\n"));
    let mut files: Vec<String> = fs::read_dir(&out)
        .expect("read out")
        .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
        .collect();
    files.sort();
    assert_eq!(
        files,
        ["frame-00003.png", "frame-00004.png", "frame-00005.png"]
    );
    let session = Session::open(LAUNCH).expect("open");
    let png = fs::read(out.join("frame-00004.png")).expect("read png");
    assert_eq!(
        png,
        session.render(4.0).expect("render").to_png().expect("png")
    );
}

#[test]
fn render_takes_one_frame_and_makes_the_folder() {
    let out = scratch("render-one").join("deeper");
    let dir = out.to_str().expect("utf-8 path");
    let run = motion(&["render", LAUNCH, "--frame", "0", "--out", dir]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run), format!("1 frame written to {dir}\n"));
    assert!(out.join("frame-00000.png").is_file());
}

#[test]
fn render_past_the_end_exits_with_the_engine_error() {
    let out = scratch("render-past");
    let dir = out.to_str().expect("utf-8 path");
    let run = motion(&["render", LAUNCH, "--frames", "149-150", "--out", dir]);
    let expected = Session::open(LAUNCH)
        .expect("open")
        .render_frames(149..151, Default::default(), |_, _| Ok(()))
        .expect_err("past the end");
    assert_eq!(
        run.status.code(),
        Some(i32::from(expected.kind.exit_code()))
    );
    assert_eq!(stderr(&run), error_line(&expected));
}

#[rstest]
#[case::both_frame_flags(&["--frame", "1", "--frames", "1-2"])]
#[case::a_backwards_range(&["--frames", "5-1"])]
fn bad_render_arguments_are_usage_errors(#[case] args: &[&str]) {
    let out = scratch(&format!("render-usage-{}", args.len()));
    let mut all = vec!["render", LAUNCH, "--out", out.to_str().expect("utf-8")];
    all.extend_from_slice(args);
    let run = motion(&all);
    assert_eq!(run.status.code(), Some(2));
    assert!(stdout(&run).is_empty());
}
