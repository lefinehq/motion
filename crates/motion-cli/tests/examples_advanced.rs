//! The advanced examples in `examples/advanced/`: every scene there passes
//! `motion check` with nothing found, and the variants example's batch makes
//! every variant.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use pretty_assertions::assert_eq;
use rstest::rstest;

const ADVANCED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/advanced");

/// The batch file of the variants example, which is not a scene.
const BATCH: &str = "variants/batch.json";

fn motion(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_motion"))
        .env_remove("MOTION_LOG")
        .args(args)
        .output()
        .expect("run motion")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Every scene under `dir`, at any depth, leaving out batch files and the
/// folders a batch writes its variants into.
fn scenes(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read the examples") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "out") {
                scenes(&path, found);
            }
        } else if path.extension().is_some_and(|ext| ext == "json") && !path.ends_with(BATCH) {
            found.push(path);
        }
    }
}

/// The scenes, relative to `examples/advanced/`.
const SCENES: [&str; 5] = [
    "camera-depth.json",
    "kinetic-type.json",
    "shape-mark.json",
    "ui-reveal.json",
    "variants/master.json",
];

#[test]
fn the_list_holds_every_scene_of_the_folder() {
    let mut found = Vec::new();
    scenes(Path::new(ADVANCED), &mut found);
    let mut found: Vec<String> = found
        .iter()
        .map(|path| {
            let relative = path.strip_prefix(ADVANCED).expect("inside the folder");
            relative.to_string_lossy().replace('\\', "/")
        })
        .collect();
    found.sort();
    assert_eq!(found, SCENES);
}

#[rstest]
#[case::camera_depth(SCENES[0])]
#[case::kinetic_type(SCENES[1])]
#[case::shape_mark(SCENES[2])]
#[case::ui_reveal(SCENES[3])]
#[case::variants_master(SCENES[4])]
fn an_advanced_example_passes_check_with_nothing_found(#[case] scene: &str) {
    let out = motion(&["check", &format!("{ADVANCED}/{scene}")]);
    let text = stdout(&out);
    assert!(
        text.starts_with("OK ") && text.lines().count() == 1,
        "{scene}: {text}"
    );
    assert_eq!(out.status.code(), Some(0), "{scene}");
}

#[test]
fn the_variants_batch_checks_every_variant() {
    let manifest = format!("{ADVANCED}/{BATCH}");
    let out = motion(&["batch", &manifest, "--check"]);
    let text = stdout(&out);
    assert!(
        text.ends_with("4 checked, 0 refused, 0 failed their check\n"),
        "{text}"
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
}

#[test]
fn the_variants_batch_renders_every_variant() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("examples-advanced-batch");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the scratch folder");

    // The example's own batch, with the master named where it lies and two
    // frames a variant, so the test renders quickly.
    let text = fs::read_to_string(format!("{ADVANCED}/{BATCH}")).expect("read the batch");
    let mut batch: serde_json::Value = serde_json::from_str(&text).expect("a JSON batch");
    batch["master"] = format!("{ADVANCED}/variants/master.json").into();
    batch["out"] = "out".into();
    for variant in batch["variants"].as_array_mut().expect("variants") {
        variant["render"] = serde_json::json!({"frames": "0-1"});
    }
    let manifest = dir.join("batch.json");
    fs::write(&manifest, batch.to_string()).expect("write the batch");

    let out = motion(&["batch", manifest.to_str().expect("utf-8 path"), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let made: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    let made = made.as_array().expect("one outcome a variant");
    assert_eq!(made.len(), 4);
    for variant in made {
        assert_eq!(variant["outcome"], "rendered", "{variant}");
        let id = variant["id"].as_str().expect("an id");
        assert!(dir.join(format!("out/{id}.mp4")).exists(), "{id}");
    }
}
