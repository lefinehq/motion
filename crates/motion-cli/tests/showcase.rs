//! The showcase scenes in `examples/showcase/`: each passes `motion check`
//! with nothing found. Each scene has its own test and helpers, named after it.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// `examples/showcase/brand-system.json`, the brand-identity film.
const BRAND_SYSTEM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/showcase/brand-system.json"
);

/// Runs `motion` with `args`, without a `MOTION_LOG` from the environment.
fn brand_system_motion(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_motion"))
        .env_remove("MOTION_LOG")
        .args(args)
        .output()
        .expect("motion runs")
}

/// A batch of one variant without edits of the brand-system film, written
/// to a folder of its own; `--check` applies and checks it, rendering nothing.
fn brand_system_batch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motion-showcase-brand-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let master = fs::canonicalize(BRAND_SYSTEM).expect("the scene exists");
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

/// The brand-system film passes every rule and gate with nothing found.
///
/// `motion check` also runs the pixel gates, `contrast` and `flash`, which
/// draw the frames they sample. For this 32 s 1080p film with 3D planes and
/// motion blur that takes about 30 s in a release build and over ten minutes
/// in a debug one. So a debug build checks the scene's rules alone, through
/// `motion batch --check` (about a minute of one core), and a release build
/// (`cargo nextest run --release`) runs the whole `motion check`.
#[test]
fn brand_system_passes_check() {
    if cfg!(debug_assertions) {
        let batch = brand_system_batch();
        let out = brand_system_motion(&["batch", &batch.to_string_lossy(), "--check"]);
        let text = String::from_utf8_lossy(&out.stdout);
        let _ = fs::remove_dir_all(batch.parent().expect("batch dir"));
        assert!(
            text.lines()
                .any(|l| l.starts_with("brand-system") && l.ends_with("check OK"))
                && text.contains("1 checked, 0 refused, 0 failed their check"),
            "{text}"
        );
        assert_eq!(out.status.code(), Some(0));
    } else {
        let out = brand_system_motion(&["check", BRAND_SYSTEM]);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.starts_with("OK ") && text.lines().count() == 1,
            "{text}"
        );
        assert_eq!(out.status.code(), Some(0));
    }
}
