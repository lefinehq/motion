//! End-to-end tests: run the real `nakkash` binary and check its output and
//! exit code. An error line is compared with the engine's own diagnostic, so
//! only the `error: …` format is pinned here, not the wording.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use nakkash_engine::{Diagnostic, Session};
use pretty_assertions::assert_eq;
use rstest::rstest;
use sha2::Digest;

const LAUNCH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/launch.json");
const OFFSCREEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/offscreen.json");
const SLOTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../nakkash-format/tests/compat/v1/slots.json"
);

/// Runs `nakkash` with the given arguments, without a `NAKKASH_LOG` from the
/// environment.
fn nakkash(args: &[&str]) -> Output {
    run(
        Command::new(env!("CARGO_BIN_EXE_nakkash")).env_remove("NAKKASH_LOG"),
        args,
    )
}

/// Runs `nakkash` with `NAKKASH_LOG` set to `filter`.
fn nakkash_with_log(filter: &str, args: &[&str]) -> Output {
    run(
        Command::new(env!("CARGO_BIN_EXE_nakkash")).env("NAKKASH_LOG", filter),
        args,
    )
}

fn run(command: &mut Command, args: &[&str]) -> Output {
    command.args(args).output().expect("run nakkash")
}

/// The line `nakkash` prints to stderr for `diagnostic`.
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
    let out = nakkash(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(
        text.starts_with(
            "Check, query, edit and render nakkash scenes, and inspect `.aep` projects\n"
        ),
        "{text}"
    );
    for command in [
        "check", "at", "roles", "slots", "edit", "render", "batch", "inspect",
    ] {
        assert!(
            text.lines().any(|l| l.trim_start().starts_with(command)),
            "{command}: {text}"
        );
    }
}

#[test]
fn check_clean_example_exits_zero() {
    let out = nakkash(&["check", LAUNCH]);
    assert_eq!(stdout(&out), "OK 12\n");
    assert_eq!(out.status.code(), Some(0));
}

/// The agent skill is the guide with its links made absolute, so an agent
/// that has only the skill reads the same text.
#[rstest]
#[case::from_scratch("from-scratch", "nakkash")]
#[case::template("fill-a-template", "nakkash-template")]
fn the_skill_is_the_guide(#[case] guide: &str, #[case] skill: &str) {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let guide = fs::read_to_string(format!("{root}/docs/guide/{guide}.md")).expect("guide");
    let skill = fs::read_to_string(format!("{root}/skills/{skill}/SKILL.md")).expect("skill");
    let base = "https://github.com/lefinehq/nakkash/blob/main/";
    let linked = guide
        .replace("](../../examples/", &format!("]({base}examples/"))
        .replace("](../design/", &format!("]({base}docs/design/"));
    let body = skill
        .splitn(3, "---\n")
        .nth(2)
        .expect("the skill starts with frontmatter")
        .trim_start_matches('\n');
    assert_eq!(body, linked, "regenerate the skill from its guide");
}

/// The scenes written from scratch in `examples/`, which agents read as a
/// guide: each passes every rule and gate with nothing found.
#[rstest]
#[case::social("social-916.json")]
#[case::lower_third("lower-third.json")]
#[case::title_card("title-card.json")]
#[case::instance_template("instance/promo.json")]
#[case::styled_cards("styled-cards.json")]
fn an_example_written_from_scratch_passes_check(#[case] name: &str) {
    let path = format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"));
    let out = nakkash(&["check", &path]);
    let text = stdout(&out);
    assert!(
        text.starts_with("OK ") && text.lines().count() == 1,
        "{name}: {text}"
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn check_offscreen_exits_one() {
    let out = nakkash(&["check", OFFSCREEN]);
    assert!(stdout(&out).starts_with("E offscreen card 0-149f +120px"));
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn at_accepts_seconds_and_frames() {
    let secs = nakkash(&["at", LAUNCH, "1s", "title"]);
    let frames = nakkash(&["at", LAUNCH, "30f", "title"]);
    assert_eq!(secs.status.code(), Some(0));
    assert!(!stdout(&secs).is_empty());
    assert_eq!(stdout(&secs), stdout(&frames));
}

#[test]
fn at_without_ids_prints_every_node() {
    let out = nakkash(&["at", LAUNCH, "1s"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.contains("title") && text.contains("card"), "{text}");
}

#[rstest]
#[case::unknown_id("1s", "nope")]
#[case::bad_time("soon", "title")]
fn a_failing_at_exits_two(#[case] time: &str, #[case] id: &str) {
    let out = nakkash(&["at", LAUNCH, time, id]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stderr(&out), error_line(&at_error(time, &[id])));
}

#[test]
fn missing_file_exits_two() {
    let missing = "/definitely/not/here.json";
    let out = nakkash(&["check", missing]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stderr(&out), error_line(&open_error(missing)));
}

#[test]
fn edit_writes_canonical_file_and_prints_report() {
    let path = copy_to("edit_canonical", OFFSCREEN);
    let out = nakkash(&["edit", &path, "card at=\"center 700\""]);
    assert_eq!(stdout(&out), "OK 2\n");
    assert_eq!(out.status.code(), Some(0));
    let text = fs::read_to_string(&path).expect("read back");
    assert!(nakkash_format::parse_document(&text).is_ok());
    assert_eq!(text.lines().nth(1), Some("\"layers\":["));
}

#[test]
fn a_baseline_leaves_out_what_it_already_has() {
    let out = nakkash(&["check", OFFSCREEN, "--baseline", OFFSCREEN]);
    // The pixel gates pass.
    assert_eq!(stdout(&out), "KNOWN 1\nOK 1\n");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn an_edit_prints_only_what_it_added() {
    // The card was already off the canvas: an edit elsewhere passes.
    let path = copy_to("edit_known", OFFSCREEN);
    let out = nakkash(&["edit", &path, "card fill=#FF0000"]);
    assert_eq!(stdout(&out), "KNOWN 1\nOK 0\n");
    assert_eq!(out.status.code(), Some(0));
    // Moving it farther out is new.
    let out = nakkash(&["edit", &path, "card at=\"center 1100\""]);
    assert!(
        stdout(&out).starts_with("E offscreen card"),
        "{}",
        stdout(&out)
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn slots_lists_each_slot_and_an_edit_changes_one() {
    let path = copy_to("slots", SLOTS);
    let out = nakkash(&["slots", &path]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out).lines().take(2).collect::<Vec<_>>(),
        [
            "scene",
            r##"  $accent color "#E63946"  "Accent"  (2 values follow)"##
        ]
    );
    let out = nakkash(&["edit", &path, "$accent=#2A9D8F"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let out = nakkash(&["slots", &path, "--json"]);
    let listed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(listed[0]["value"], "#2A9D8F");
    assert_eq!(listed[0]["followers"][0], "plate.fill");
}

#[test]
fn roles_list_what_each_holds_and_an_edit_goes_through_one() {
    let path = copy_to("roles", SLOTS);
    let out = nakkash(&["roles", &path]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        "headline  tag/word  text \"Launch\"\nplate     plate     rect  locked\naccent    $accent   color \"#E63946\"\n"
    );
    let out = nakkash(&["edit", &path, "@accent=#2A9D8F"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let out = nakkash(&["roles", &path, "--json"]);
    let roles: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(roles[2]["value"], "#2A9D8F");
}

#[test]
fn a_batch_renders_the_variants_that_pass_and_names_why_the_others_do_not() {
    let dir = scratch("batch");
    fs::copy(SLOTS, dir.join("master.json")).expect("copy the master");
    let manifest = dir.join("variants.json");
    let variants = serde_json::json!({"master":"master.json","out":"out","variants":[
        {"id":"teal","edits":["@accent=#2A9D8F","@headline text=\"Sale\""],"render":{"frames":"0-1"}},
        {"id":"plate","edits":["plate fill=#2A9D8F"]},
        {"id":"unlock","edits":["scene locks=[]"]}]});
    fs::write(&manifest, variants.to_string()).expect("write the manifest");
    let manifest = manifest.to_string_lossy().into_owned();
    let out = nakkash(&["batch", &manifest, "--check"]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("teal     2 edits  check OK\n"), "{text}");
    assert!(
        text.contains("refused: `plate` is locked by `@plate` (edit 1)"),
        "{text}"
    );
    assert!(
        text.ends_with("1 checked, 2 refused, 0 failed their check\n"),
        "{text}"
    );
    assert!(!dir.join("out").exists(), "--check writes nothing");
    let out = nakkash(&["batch", &manifest, "--json"]);
    let made: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(made[0]["outcome"], "rendered");
    assert_eq!(made[0]["frames"], 2);
    assert_eq!(made[2]["outcome"], "refused");
    let teal = fs::read_to_string(dir.join("out/teal.json")).expect("the variant's scene");
    assert!(teal.contains("\"Sale\""), "{teal}");
    assert!(dir.join("out/teal.mp4").exists());
}

#[test]
fn a_batch_measures_each_variant_against_its_master() {
    let dir = scratch("batch_baseline");
    // The master already has an error: its card rests past the bottom.
    fs::copy(OFFSCREEN, dir.join("master.json")).expect("copy the master");
    let manifest = dir.join("variants.json");
    let variants = serde_json::json!({"master":"master.json","out":"out","variants":[
        {"id":"wider","edits":["card rect=\"320x400\""]},
        {"id":"lower","edits":["card at=\"center 1100\""]},
        {"id":"fixed","edits":["card at=\"center 826\""]}]});
    fs::write(&manifest, variants.to_string()).expect("write the manifest");
    let manifest = manifest.to_string_lossy().into_owned();
    let out = nakkash(&["batch", &manifest, "--check"]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    // The master's error is known, so a variant that keeps it passes.
    assert!(
        text.contains("wider    1 edit  check OK  KNOWN 1\n"),
        "{text}"
    );
    // One that makes it worse fails, naming what it added.
    assert!(
        text.contains("lower    1 edit  check failed: E offscreen card 0-149f +220px"),
        "{text}"
    );
    assert!(text.contains("fixed    1 edit  check OK\n"), "{text}");
    assert!(
        text.ends_with("2 checked, 0 refused, 1 failed their check\n"),
        "{text}"
    );
}

#[test]
fn edit_takes_nested_keys_and_json_values() {
    let path = copy_to("edit_nested_json", LAUNCH);
    let out = nakkash(&[
        "edit",
        &path,
        r#"title in=none anim.opacity=[["0f",0],["12f",1]]"#,
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = fs::read_to_string(&path).expect("read back");
    assert!(nakkash_format::parse_document(&text).is_ok());
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
    let out = nakkash(&["edit", &path, "card fill=blue"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read(&path).expect("read"), before);
}

#[test]
fn edit_malformed_line_leaves_file_untouched() {
    let path = copy_to("edit_malformed", OFFSCREEN);
    let before = fs::read(&path).expect("read");
    let out = nakkash(&["edit", &path, "card"]);
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
    let out = nakkash(&["edit", &path, "card at=\"center 1100\""]);
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
    let out = nakkash(&["check", path.to_str().expect("utf-8")]);
    assert_eq!(out.status.code(), Some(2));
    let invalid = open_error(&path);
    assert_eq!(invalid.path_text().as_deref(), Some("title.in"));
    assert_eq!(stderr(&out), error_line(&invalid));
    assert!(stderr(&out).starts_with("error: title.in: "));
}

#[test]
fn a_failure_is_logged_once_before_the_error_line() {
    let out = nakkash(&["--log", "error", "at", LAUNCH, "1s", "nope"]);
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
    let out = nakkash(args);
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(lines[0].starts_with("error: "), "{text}");
}

#[test]
fn info_logs_one_line_per_command() {
    let out = nakkash(&["--log", "info", "check", LAUNCH]);
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(
        lines[0].contains(r#"command="check""#) && lines[0].contains(&format!("file={LAUNCH}")),
        "{text}"
    );
}

#[test]
fn an_invalid_nakkash_log_is_one_warning_then_the_level() {
    let out = nakkash_with_log("nakkash=loud", &["at", LAUNCH, "1s", "nope"]);
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(
        lines[0].contains(" WARN nakkash_engine::telemetry")
            && lines[0].contains("value=nakkash=loud"),
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
    let plain = nakkash(args);
    let logged = nakkash(&[&["--log", "debug", "--log-format", "json"], args].concat());
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
    let out = nakkash(&["--log", "debug", "--log-format", "json", "check", LAUNCH]);
    let text = stderr(&out);
    assert!(
        text.lines()
            .any(|l| l.contains(r#""name":"check""#) && l.contains(r#""passed":10"#)),
        "{text}"
    );
}

#[test]
fn timings_print_one_line_on_stderr() {
    let out = nakkash(&["--timings", "check", LAUNCH]);
    assert_eq!(stdout(&out), "OK 12\n");
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
        let text = stderr(&nakkash(&[&["--timings"], args].concat()));
        let steps: Vec<&str> = text
            .strip_prefix("timings ")
            .unwrap_or_else(|| panic!("{args:?}: {text}"))
            .split_whitespace()
            .map(|step| step.split_once('=').expect("name=time").0)
            .collect();
        let json = stderr(&nakkash(
            &[&["--timings", "--log-format", "json"], args].concat(),
        ));
        let events: Vec<String> = json
            .lines()
            .map(|line| {
                let event: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
                assert_eq!(event["level"], "INFO", "{json}");
                assert_eq!(event["target"], "nakkash::timings", "{json}");
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
    let out = nakkash(&["at", LAUNCH, "soon", "--timings"]);
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
    let out = nakkash(&["inspect", &path]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(
        text.starts_with(
            "project 94.9, unknown bits per channel, expressions in extendscript\nreport\n"
        ),
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
    let out = nakkash(&["inspect", &path, "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(json["project"]["version"]["major"], 94);
    assert_eq!(json["report"]["layers"], serde_json::json!({}));
}

#[test]
fn inspect_a_file_that_is_not_a_project_exits_two() {
    let out = nakkash(&["inspect", LAUNCH]);
    assert_eq!(out.status.code(), Some(2));
    let error = nakkash_engine::import(LAUNCH).expect_err("not a project");
    assert_eq!(stderr(&out), error_line(&error));
}

#[test]
fn render_writes_one_numbered_png_per_frame() {
    let out = scratch("render-frames");
    let dir = out.to_str().expect("utf-8 path");
    let run = nakkash(&["render", LAUNCH, "--frames", "3-5", "--out", dir]);
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
fn render_digest_prints_each_frames_pixels_hash_and_writes_nothing() {
    // Run in an empty folder, which must stay empty.
    let out = scratch("render-digest");
    let run = run(
        Command::new(env!("CARGO_BIN_EXE_nakkash"))
            .env_remove("NAKKASH_LOG")
            .current_dir(&out),
        &["render", LAUNCH, "--frames", "3-4", "--digest"],
    );
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let session = Session::open(LAUNCH).expect("open");
    let expected: String = [3u32, 4]
        .iter()
        .map(|&frame| {
            let image = session.render(f64::from(frame)).expect("render");
            let hash = sha2::Sha256::digest(image.pixels());
            let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
            format!(
                "frame {frame} {}x{} sha256:{hex}\n",
                image.width(),
                image.height()
            )
        })
        .collect();
    assert_eq!(stdout(&run), expected);
    assert_eq!(fs::read_dir(&out).expect("read the folder").count(), 0);
    let both = nakkash(&["render", LAUNCH, "--frame", "1", "--digest", "--out", "x"]);
    assert_eq!(both.status.code(), Some(2), "{}", stderr(&both));
}

/// A small scene in a scratch folder, so the encoder is quick in a debug
/// build; returns the folder and the scene's path.
fn small_scene(name: &str) -> (PathBuf, String) {
    let dir = scratch(name);
    let scene = dir.join("scene.json");
    fs::write(
        &scene,
        r##"{"nakkash":1,"size":"160x90","fps":30,"dur":"1s","bg":"#0B0D12","layers":[]}"##,
    )
    .expect("write the scene");
    (dir, scene.to_str().expect("utf-8 path").to_string())
}

#[test]
fn render_to_an_mp4_file_writes_a_video() {
    let (dir, scene) = small_scene("render-video");
    let out = dir.join("small.mp4");
    let file = out.to_str().expect("utf-8 path");
    let run = nakkash(&["render", &scene, "--frames", "0-5", "--out", file]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // The platform's own codec by default.
    let codec = match cfg!(target_os = "macos") {
        true => "H.264",
        false => "AV1",
    };
    assert_eq!(
        stdout(&run),
        format!("6 frames written to {file} ({codec}, web, 160x90, 30 fps, 0.2 s)\n")
    );
    let bytes = fs::read(&out).expect("read the video");
    // The file type, then the index ahead of the samples.
    assert_eq!(&bytes[4..8], b"ftyp");
    assert_eq!(&bytes[36..40], b"moov");
    let mut names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["scene.json", "small.mp4"],
        "nothing but the video is left"
    );
}

#[test]
fn render_to_a_wav_file_writes_the_mix() {
    let dir = scratch("render-mix");
    let sound = fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nakkash-format/tests/compat/v1/sounds/tick.wav"
    ))
    .expect("read the tick");
    fs::write(dir.join("tick.wav"), sound).expect("write the tick");
    let scene = dir.join("scene.json");
    fs::write(
        &scene,
        r#"{"nakkash":1,"fps":10,"dur":"1s","layers":[
          {"id":"a","audio":"tick.wav"},{"id":"b","audio":"tick.wav","offset":"0.5s","volume":4}]}"#,
    )
    .expect("write the scene");
    let out = dir.join("mix.wav");
    let file = out.to_str().expect("utf-8 path");
    let run = nakkash(&["render", scene.to_str().expect("utf-8 path"), "--out", file]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // A tick at -12 dB played four times as loud reaches just past full scale.
    assert_eq!(
        stdout(&run),
        format!(
            "mix written to {file} (1 s, 48000 Hz stereo, peak 0.0 dBFS, above full scale: lower a `volume`)\n"
        )
    );
    let mix = nakkash_audio::open(&out).expect("reads back");
    assert_eq!(
        (mix.rate(), mix.channels().len(), mix.frames()),
        (48_000, 2, 48_000)
    );
}

/// A one-second scene at 10 fps that plays the test tick, in its own folder.
fn ticking(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    let sound = fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nakkash-format/tests/compat/v1/sounds/tick.wav"
    ))
    .expect("read the tick");
    fs::write(dir.join("tick.wav"), sound).expect("write the tick");
    let scene = dir.join("scene.json");
    fs::write(
        &scene,
        r##"{"nakkash":1,"size":"64x48","fps":10,"dur":"1s","layers":[
          {"id":"card","rect":"20x20","fill":"#FFFFFF","at":"center center"},{"id":"tick","audio":"tick.wav"}]}"##,
    )
    .expect("write the scene");
    (dir, scene)
}

#[cfg(target_os = "macos")]
#[test]
fn a_video_of_a_scene_with_sound_holds_the_mix() {
    let (dir, scene) = ticking("render-sound");
    for name in ["tick.mp4", "tick.mov"] {
        let out = dir.join(name);
        let file = out.to_str().expect("utf-8 path");
        let run = nakkash(&["render", scene.to_str().expect("utf-8 path"), "--out", file]);
        assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
        assert!(
            stdout(&run).ends_with(", 1 s, with sound)\n"),
            "{}",
            stdout(&run)
        );
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn an_mp4_of_a_scene_with_sound_needs_an_aac_encoder() {
    let (dir, scene) = ticking("render-no-aac");
    let out = dir.join("tick.mp4");
    let run = nakkash(&[
        "render",
        scene.to_str().expect("utf-8 path"),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_ne!(run.status.code(), Some(0));
    assert!(stderr(&run).contains("no AAC encoder"), "{}", stderr(&run));
}

#[test]
fn render_takes_a_codec_and_a_video_quality_and_times_the_encoding() {
    let (dir, scene) = small_scene("render-video-quality");
    let file = dir.join("draft.mp4");
    let file = file.to_str().expect("utf-8 path");
    let args = [
        "--timings",
        "render",
        &scene,
        "--frames",
        "0-2",
        "--out",
        file,
    ];
    let run = nakkash(&[&args[..], &["--codec", "av1", "--video-quality", "draft"]].concat());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("(AV1, draft, 160x90"),
        "{}",
        stdout(&run)
    );
    let timings = stderr(&run);
    assert!(timings.contains("encode"), "{timings}");
}

#[rstest]
#[case::codec(&["--codec", "h265"], "h265")]
#[case::quality(&["--video-quality", "best"], "best")]
fn render_refuses_a_codec_or_quality_it_lacks(#[case] extra: &[&str], #[case] name: &str) {
    let (dir, scene) = small_scene(&format!("render-video-{name}"));
    let file = dir.join("a.mp4");
    let args = [
        "render",
        &scene,
        "--out",
        file.to_str().expect("utf-8 path"),
    ];
    let run = nakkash(&[&args[..], extra].concat());
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stderr(&run).contains(name), "{}", stderr(&run));
    assert!(!file.exists());
}

#[test]
fn render_refuses_a_codec_for_png_frames() {
    let (dir, scene) = small_scene("render-png-codec");
    let frames = dir.join("frames");
    let out = frames.to_str().expect("utf-8 path");
    let run = nakkash(&["render", &scene, "--out", out, "--codec", "av1"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stderr(&run).contains("--codec"), "{}", stderr(&run));
    assert!(!frames.exists());
}

#[test]
fn render_takes_one_frame_and_makes_the_folder() {
    let out = scratch("render-one").join("deeper");
    let dir = out.to_str().expect("utf-8 path");
    let run = nakkash(&["render", LAUNCH, "--frame", "0", "--out", dir]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run), format!("1 frame written to {dir}\n"));
    assert!(out.join("frame-00000.png").is_file());
}

#[test]
fn render_past_the_end_exits_with_the_engine_error() {
    let out = scratch("render-past");
    let dir = out.to_str().expect("utf-8 path");
    let run = nakkash(&["render", LAUNCH, "--frames", "149-150", "--out", dir]);
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
    let run = nakkash(&all);
    assert_eq!(run.status.code(), Some(2));
    assert!(stdout(&run).is_empty());
}

/// A fixture project from nakkash-import's own.
const HAMBURGER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../nakkash-import/tests/fixtures/HamburgerArrow.aep"
);

#[test]
fn export_writes_an_unedited_scene_back_unchanged_and_an_edit_into_the_project() {
    let dir = scratch("export");
    let scene = dir.join("scene.json");
    let out = nakkash(&["import", HAMBURGER, "--out", scene.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let same = dir.join("same.aep");
    let out = nakkash(&[
        "export",
        scene.to_str().unwrap(),
        "--project",
        HAMBURGER,
        "--out",
        same.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("0 written back, 0 not"),
        "{}",
        stdout(&out)
    );
    assert_eq!(fs::read(&same).unwrap(), fs::read(HAMBURGER).unwrap());

    let text = fs::read_to_string(&scene).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let rotator = json["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|l| l["id"] == "rotator")
        .unwrap();
    rotator["anchor"] = serde_json::json!([0.25, 0.75]);
    fs::write(&scene, json.to_string()).unwrap();
    let edited = dir.join("edited.aep");
    let out = nakkash(&[
        "export",
        scene.to_str().unwrap(),
        "--project",
        HAMBURGER,
        "--out",
        edited.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("1 written back, 0 not"),
        "{}",
        stdout(&out)
    );
    let back = dir.join("back.json");
    let out = nakkash(&[
        "import",
        edited.to_str().unwrap(),
        "--out",
        back.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0));
    let back: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&back).unwrap()).unwrap();
    let rotator = back["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == "rotator")
        .unwrap();
    assert_eq!(rotator["anchor"], serde_json::json!([0.25, 0.75]));
}

#[test]
fn a_mogrt_is_opened_as_a_template_and_cannot_be_exported_into() {
    let dir = scratch("mogrt");
    let template = dir.join("bad.mogrt");
    fs::write(&template, b"RIFX, not an archive").unwrap();
    let scene = dir.join("scene.json");
    let out = nakkash(&[
        "import",
        template.to_str().unwrap(),
        "--out",
        scene.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(
        stderr(&out).contains("the file is not a zip archive"),
        "{}",
        stderr(&out)
    );
    let out = nakkash(&["import", HAMBURGER, "--out", scene.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let out = nakkash(&[
        "export",
        scene.to_str().unwrap(),
        "--project",
        template.to_str().unwrap(),
        "--out",
        dir.join("out.mogrt").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(
        stderr(&out).contains("cannot be written back into a `.mogrt` template yet"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn bench_runs_the_agent_on_each_brief_and_records_what_check_finds() {
    let dir = scratch("bench");
    let briefs = dir.join("briefs");
    fs::create_dir_all(briefs.join("social")).unwrap();
    fs::write(briefs.join("social/01-card.md"), "# A card\n").unwrap();
    fs::write(briefs.join("social/02-nothing.md"), "# Nothing\n").unwrap();
    // The fake agent writes a small scene for the card brief only, then its cost.
    let agent = r##"grep -q card "$NAKKASH_BRIEF" && printf '%s' '{"nakkash":1,"size":"320x180","dur":"10f","layers":[{"id":"card","rect":"100x60","at":"center 175"}]}' > scene.json; echo '{"tokens":42,"steps":3}'"##;
    let out = dir.join("runs");
    let run = nakkash(&[
        "bench",
        briefs.to_str().unwrap(),
        "--agent",
        agent,
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let lines: Vec<serde_json::Value> = fs::read_to_string(out.join("results.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let [card, nothing] = &lines[..] else {
        panic!("two records: {lines:?}");
    };
    assert_eq!(
        (
            &card["brief"],
            &card["tokens"],
            &card["steps"],
            &card["scene"],
            &card["errors"],
            &card["findings"],
            &card["video_seconds"],
        ),
        (
            &serde_json::json!("social/01-card"),
            &serde_json::json!(42),
            &serde_json::json!(3),
            &serde_json::json!(true),
            &serde_json::json!(1),
            &serde_json::json!({"offscreen": 1}),
            &serde_json::json!(0.33),
        )
    );
    assert!(out.join("social/01-card/video.mp4").is_file());
    assert_eq!(
        (&nothing["scene"], &nothing["errors"]),
        (&serde_json::json!(false), &serde_json::Value::Null)
    );
    assert!(
        stdout(&run).contains("social/02-nothing  agent "),
        "{}",
        stdout(&run)
    );
}

/// A scene in a scratch folder that still writes its version under the old
/// key, `motion` (D73); returns its path.
fn old_key_scene(name: &str) -> String {
    let scene = scratch(name).join("scene.json");
    fs::write(
        &scene,
        r#"{"motion":1,"size":"160x90","dur":"1s","layers":[{"id":"card","rect":"40x40"}]}"#,
    )
    .expect("write the scene");
    scene.to_str().expect("utf-8 path").to_string()
}

#[test]
fn a_scene_with_the_old_version_key_works_and_says_it_is_deprecated() {
    let scene = old_key_scene("old-key");
    for args in [vec!["check", &scene], vec!["at", &scene, "0s", "card"]] {
        let out = nakkash(&args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {}", stderr(&out));
        let err = stderr(&out);
        assert!(
            err.contains("nakkash::deprecated")
                && err.contains("`motion` is the old name of the scene key `nakkash`"),
            "{args:?}: {err}"
        );
    }
    let fresh = nakkash(&["check", LAUNCH]);
    assert!(!stderr(&fresh).contains("deprecated"), "{}", stderr(&fresh));
}

#[test]
fn the_old_command_name_runs_nakkash_and_says_it_is_deprecated() {
    let out = run(
        Command::new(env!("CARGO_BIN_EXE_motion")).env_remove("NAKKASH_LOG"),
        &["at", LAUNCH, "1s", "title"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        stdout(&nakkash(&["at", LAUNCH, "1s", "title"]))
    );
    assert!(
        stderr(&out).contains("`motion` is the old name of this command"),
        "{}",
        stderr(&out)
    );
    let failing = run(
        Command::new(env!("CARGO_BIN_EXE_motion")).env_remove("NAKKASH_LOG"),
        &["check", "absent.json"],
    );
    assert_eq!(
        failing.status.code(),
        nakkash(&["check", "absent.json"]).status.code()
    );
}

#[test]
fn the_old_log_variable_still_filters_and_says_it_is_deprecated() {
    let out = run(
        Command::new(env!("CARGO_BIN_EXE_nakkash"))
            .env_remove("NAKKASH_LOG")
            .env("MOTION_LOG", "info"),
        &["check", LAUNCH],
    );
    let err = stderr(&out);
    assert!(
        err.contains("MOTION_LOG is the old name of NAKKASH_LOG") && err.contains("running"),
        "{err}"
    );
}

#[test]
fn vary_makes_seeded_palettes_each_checked_against_the_master() {
    let dir = scratch("vary_palette");
    // Small and short, so its pixel gates are quick in a debug build.
    let card = dir.join("card.json");
    fs::write(
        &card,
        r##"{"nakkash":1,"size":"320x180","fps":10,"dur":"1s","bg":"#F4F1EA","layers":[
        {"id":"card","rect":"200x100","fill":"#1D3557","at":"center center"},
        {"id":"title","text":"Quiet","font":"Inter Variable 32 800","fill":"#FFFFFF","at":"center center"}]}"##,
    )
    .expect("write the master");
    let card = card.to_string_lossy().into_owned();
    let run = |out: &str| {
        let out = dir.join(out).to_string_lossy().into_owned();
        let o = nakkash(&[
            "vary",
            &card,
            "--seed",
            "7",
            "-n",
            "3",
            "--axis",
            "palette",
            "--dry-run",
            "--out",
            &out,
        ]);
        (o, PathBuf::from(out))
    };
    let (first, a) = run("a");
    let text = stdout(&first);
    assert_eq!(first.status.code(), Some(0), "{text}{}", stderr(&first));
    assert!(text.contains("3 passed, 0 failed"), "{text}");
    for id in ["v1", "v2", "v3"] {
        assert!(a.join(id).join("scene.json").is_file(), "{id}");
        assert!(a.join(id).join("check.txt").is_file(), "{id}");
    }
    assert!(a.join("master.check.txt").is_file());
    // The same seed gives the same scenes; a dry run renders nothing.
    let (_, b) = run("b");
    for id in ["v1", "v2", "v3"] {
        let read = |d: &Path| fs::read_to_string(d.join(id).join("scene.json")).expect("a scene");
        assert_eq!(read(&a), read(&b), "{id}");
        assert!(!a.join(id).join("film.mp4").exists(), "{id}");
    }
    // A variant's colours are not the master's.
    let master = fs::read_to_string(&card).expect("the master");
    let v1 = fs::read_to_string(a.join("v1").join("scene.json")).expect("v1");
    assert!(
        master.contains("#1D3557") && !v1.contains("#1D3557"),
        "{v1}"
    );
}

#[test]
fn vary_without_an_axis_says_what_to_ask_for() {
    let out = nakkash(&["vary", LAUNCH, "--dry-run"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("no axis to vary along"),
        "{}",
        stderr(&out)
    );
}

/// A scene split into a file per composition, and flattened back, reads and
/// draws as the scene (D91); a split scene is never saved over as one file.
#[test]
fn a_split_scene_draws_as_the_scene_and_flattens_back() {
    let scene = format!(
        "{}/../../examples/social-916.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let dir = std::env::temp_dir().join(format!("nakkash-split-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let split = dir.join("scene.json");
    let flat = dir.join("flat.json");
    let split_path = split.to_str().unwrap();
    let flat_path = flat.to_str().unwrap();
    assert_eq!(
        nakkash(&["split", &scene, "-o", split_path]).status.code(),
        Some(0)
    );
    let top: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&split).unwrap()).unwrap();
    assert_eq!(
        top["comps"]["hook"],
        serde_json::json!({"include":"comps/hook.json"})
    );
    assert!(dir.join("comps/end.json").exists());
    assert_eq!(
        nakkash(&["flatten", split_path, "-o", flat_path])
            .status
            .code(),
        Some(0)
    );
    let digests = |path: &str| {
        stdout(&nakkash(&[
            "render", path, "--frames", "118-121", "--digest",
        ]))
    };
    let want = digests(&scene);
    assert!(!want.is_empty());
    assert_eq!(digests(split_path), want);
    assert_eq!(digests(flat_path), want);
    let original: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&scene).unwrap()).unwrap();
    let flattened: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&flat).unwrap()).unwrap();
    // Key order too, which the scene reads and its hash covers.
    assert_eq!(flattened.to_string(), original.to_string());
    // The split example is that split, kept in step.
    let example = format!(
        "{}/../../examples/split/social-916.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let example_flat = dir.join("example-flat.json");
    let example_flat = example_flat.to_str().unwrap();
    assert_eq!(
        nakkash(&["flatten", &example, "-o", example_flat])
            .status
            .code(),
        Some(0)
    );
    let example_flat: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(example_flat).unwrap()).unwrap();
    assert_eq!(example_flat.to_string(), original.to_string());
    let edit = nakkash(&["edit", split_path, "hook hidden=true"]);
    assert_ne!(edit.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&edit.stderr).contains("flatten"));
    let _ = fs::remove_dir_all(&dir);
}

/// A template's hash is of the one scene it reads as, so splitting it
/// leaves the pin of its instances as it was (D91).
#[test]
fn a_split_template_keeps_its_hash() {
    let template = format!(
        "{}/../../examples/instance/promo.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let dir = std::env::temp_dir().join(format!("nakkash-split-pin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let split = dir.join("promo.json");
    let split_path = split.to_str().unwrap();
    assert_eq!(
        nakkash(&["split", &template, "-o", split_path])
            .status
            .code(),
        Some(0)
    );
    let pin = |path: &str| {
        let out = nakkash(&["slots", path, "--as-instance"]);
        let instance: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("an instance");
        instance["template"]["hash"].clone()
    };
    let hash = pin(&template);
    assert!(
        hash.as_str().is_some_and(|h| h.starts_with("sha256:")),
        "{hash}"
    );
    assert_eq!(pin(split_path), hash);
    let _ = fs::remove_dir_all(&dir);
}
