//! The showcase sequences in `examples/showcase/`: long, dense films written
//! from scratch, each of which passes `nakkash check` with nothing found.
//!
//! `nakkash check` runs the scene's rules over every frame, which takes
//! seconds, and then its pixel gates (`contrast`, `flash`), which draw every
//! frame: about half a minute per film in a release build, and from ten
//! minutes to over an hour in a debug one, where our own crates are not
//! optimised. So a debug build runs each film's rules on every frame, and a
//! release build (`cargo nextest run --release`) runs the whole command and
//! holds it to a single `OK` line and exit code 0.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use nakkash_engine::Session;

/// The path of `examples/showcase/{name}`.
fn showcase(name: &str) -> String {
    format!(
        "{}/../../examples/showcase/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Runs `nakkash` with `args`, without a `NAKKASH_LOG` from the environment.
fn nakkash(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nakkash"))
        .env_remove("NAKKASH_LOG")
        .args(args)
        .output()
        .expect("nakkash runs")
}

/// The scene's rules find nothing over every frame.
fn rules_pass(name: &str) {
    let report = Session::open(showcase(name))
        .unwrap_or_else(|e| panic!("open {name}: {e}"))
        .check()
        .unwrap_or_else(|e| panic!("check {name}: {e}"));
    assert!(report.violations.is_empty(), "{name}:\n{report}");
    assert!(report.passed > 0, "{name}: the rules ran on nothing");
}

/// `nakkash check`, gates included, prints a single `OK` line and exits 0.
fn check_passes(name: &str) {
    let out = nakkash(&["check", &showcase(name)]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("OK ") && text.lines().count() == 1,
        "{name}:\n{text}"
    );
    assert_eq!(out.status.code(), Some(0), "{name}");
}

/// The product launch film (1172 frames) passes `nakkash check`; its gates
/// take about 16 minutes in a debug build.
#[test]
fn product_launch_passes_check() {
    if cfg!(debug_assertions) {
        rules_pass("product-launch.json");
    } else {
        check_passes("product-launch.json");
    }
}

/// A batch of one variant without edits of the brand-system film, written
/// to a folder of its own; `--check` applies and checks it, rendering nothing.
fn brand_system_batch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nakkash-showcase-brand-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let master = fs::canonicalize(showcase("brand-system.json")).expect("the scene exists");
    let batch = brand_system_json_string(&master.to_string_lossy());
    let path = dir.join("batch.json");
    fs::write(
        &path,
        format!(
            r#"{{"master":{batch},"out":"out","variants":[{{"id":"brand-system","edits":[]}}]}}"#
        ),
    )
    .expect("write batch");
    path
}

/// `text` as a JSON string.
fn brand_system_json_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The brand-system film (32 s, 3D planes and motion blur) passes `nakkash
/// check`. A debug build checks that its rules find no error, as a batch
/// of it would (three titles sit 2 px past the safe area, a warning); a
/// release build runs its gates too, and `nakkash batch --check` on it,
/// which proves the film works as a master: a batch gates each variant by
/// the full report, gates included, which a debug build would take most of
/// an hour over.
#[test]
fn brand_system_passes_check() {
    if cfg!(debug_assertions) {
        let report = Session::open(showcase("brand-system.json"))
            .and_then(|s| s.check())
            .expect("check brand-system.json");
        assert!(!report.has_errors(), "brand-system.json:\n{report}");
        assert!(report.passed > 0, "the rules ran on nothing");
        return;
    }
    check_passes("brand-system.json");
    let batch = brand_system_batch();
    let out = nakkash(&["batch", &batch.to_string_lossy(), "--check"]);
    let text = String::from_utf8_lossy(&out.stdout);
    let _ = fs::remove_dir_all(batch.parent().expect("batch dir"));
    assert!(
        text.lines()
            .any(|l| l.starts_with("brand-system") && l.ends_with("check OK"))
            && text.contains("1 checked, 0 refused, 0 failed their check"),
        "{text}"
    );
    assert_eq!(out.status.code(), Some(0));
}

/// The data story (1176 frames, one page read by a camera) passes `nakkash
/// check`.
#[test]
fn data_story_passes_check() {
    if cfg!(debug_assertions) {
        rules_pass("data-story.json");
    } else {
        check_passes("data-story.json");
    }
}
