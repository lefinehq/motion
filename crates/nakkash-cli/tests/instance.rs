//! Instances on the command line ([D91]): an instance of a template's own
//! values draws the template bit for bit, every problem comes back at once
//! as an object, an edit is written only when nothing is refused, and the
//! template is never written.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value as Json, json};

const PROMO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/instance/promo.json"
);
const PROMO_DE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/instance/promo-de.json"
);

/// A frame of each section of the example.
const FRAMES: [&str; 4] = ["20", "90", "140", "175"];

fn nakkash(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nakkash"))
        .env_remove("NAKKASH_LOG")
        .args(args)
        .output()
        .expect("run nakkash")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A fresh folder holding a copy of the example template.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("instance")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch");
    fs::copy(PROMO, dir.join("promo.json")).expect("copy the template");
    dir
}

fn path(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

/// The digests of the example's section frames for `scene`.
fn digests(scene: &Path) -> Vec<String> {
    FRAMES
        .iter()
        .map(|frame| {
            let out = nakkash(&["render", path(scene), "--frame", frame, "--digest"]);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            stdout(&out)
        })
        .collect()
}

/// An instance pinned to `dir/promo.json`, written as `dir/<name>`.
fn pinned(dir: &Path, name: &str, all: bool) -> PathBuf {
    let out = dir.join(name);
    let template = dir.join("promo.json");
    let mut args = vec![
        "slots",
        path(&template),
        "--as-instance",
        "--out",
        path(&out),
    ];
    if all {
        args.push("--all");
    }
    let run = nakkash(&args);
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    out
}

fn read(path: &Path) -> Json {
    serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("JSON")
}

fn write(path: &Path, json: &Json) {
    fs::write(path, serde_json::to_string_pretty(json).expect("JSON")).expect("write");
}

#[test]
fn an_instance_of_the_templates_own_values_draws_the_template_bit_for_bit() {
    let dir = scratch("same");
    let template = digests(&dir.join("promo.json"));
    let empty = pinned(&dir, "empty.json", false);
    assert_eq!(read(&empty)["slots"], json!({}));
    assert_eq!(digests(&empty), template, "an empty instance");
    let all = pinned(&dir, "all.json", true);
    assert!(
        read(&all)["slots"]
            .as_object()
            .is_some_and(|s| s.len() == 10)
    );
    assert_eq!(
        digests(&all),
        template,
        "an instance of every value as it is"
    );
}

#[test]
fn the_example_instance_is_pinned_to_the_example_and_checks_clean() {
    let out = nakkash(&["check", PROMO_DE, "--json"]);
    let report: Json = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(report["problems"], json!([]), "{report}");
    assert_eq!(out.status.code(), Some(0), "{report}");
}

#[test]
fn check_returns_every_problem_at_once_as_objects() {
    let dir = scratch("problems");
    let at = pinned(&dir, "de.json", false);
    let mut instance = read(&at);
    instance["slots"] =
        json!({"headlin": "Hallo", "duration": 20, "ending": "end-right", "text": {"$hide": true}});
    write(&at, &instance);
    let out = nakkash(&["check", path(&at), "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let report: Json = serde_json::from_str(&stdout(&out)).expect("JSON");
    let codes: Vec<&str> = report["problems"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|p| p["code"].as_str().expect("a code"))
        .collect();
    assert_eq!(
        codes,
        ["slot.unknown"],
        "a stage with an error stops the next"
    );
    assert!(
        report["problems"][0]["hint"]
            .as_str()
            .is_some_and(|h| h.contains("`headline`"))
    );

    instance["slots"] = json!({"duration": 20, "ending": "end-right", "cta": 3});
    write(&at, &instance);
    let report: Json =
        serde_json::from_str(&stdout(&nakkash(&["check", path(&at), "--json"]))).expect("JSON");
    let problems = report["problems"].as_array().expect("a list");
    let codes: Vec<&str> = problems
        .iter()
        .map(|p| p["code"].as_str().expect("a code"))
        .collect();
    assert_eq!(
        codes,
        ["value.out_of_range", "choice.unknown", "type.mismatch"]
    );
    assert_eq!(problems[0]["measured"], json!(20));
    assert_eq!(problems[0]["limit"], json!({"min": 5.0, "max": 8.0}));
    assert_eq!(problems[0]["slot"], json!("duration"));

    let render = nakkash(&["render", path(&at), "--frame", "0", "--digest"]);
    assert_ne!(
        render.status.code(),
        Some(0),
        "an instance with errors does not render"
    );
}

#[test]
fn an_edit_is_written_only_when_nothing_is_refused_and_the_template_never() {
    let dir = scratch("edit");
    let at = pinned(&dir, "de.json", false);
    let template = fs::read(dir.join("promo.json")).expect("read");
    let out = nakkash(&[
        "edit",
        path(&at),
        "$headline=\"Jeden Markt\"",
        "$ending=end-left",
        "look=light",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    let written = read(&at);
    assert_eq!(
        written["slots"],
        json!({"headline": "Jeden Markt", "ending": "end-left"})
    );
    assert_eq!(written["look"], json!("light"));

    let refused = nakkash(&["edit", path(&at), "$duration=20"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        stdout(&refused).contains("value.out_of_range"),
        "{}",
        stdout(&refused)
    );
    assert_eq!(read(&at), written, "nothing written");

    let back = nakkash(&["edit", path(&at), "$headline=none"]);
    assert_eq!(back.status.code(), Some(0));
    assert_eq!(
        read(&at)["slots"],
        json!({"ending": "end-left"}),
        "back to the template's value, left out"
    );
    assert_eq!(
        fs::read(dir.join("promo.json")).expect("read"),
        template,
        "the template is never written"
    );
}

#[test]
fn a_changed_template_warns_and_migrate_pins_it_again() {
    let dir = scratch("migrate");
    let at = pinned(&dir, "de.json", false);
    let mut instance = read(&at);
    instance["slots"] = json!({"headline": "Hallo"});
    write(&at, &instance);
    let mut template = read(&dir.join("promo.json"));
    template["slots"]["cta"]["text"] = json!("Try it");
    write(&dir.join("promo.json"), &template);
    let report: Json =
        serde_json::from_str(&stdout(&nakkash(&["check", path(&at), "--json"]))).expect("JSON");
    assert_eq!(report["problems"][0]["code"], json!("template.changed"));
    assert_eq!(report["problems"][0]["level"], json!("warning"));
    let out = nakkash(&["migrate", path(&at)]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let report: Json =
        serde_json::from_str(&stdout(&nakkash(&["check", path(&at), "--json"]))).expect("JSON");
    assert_eq!(report["problems"], json!([]));
}

#[test]
fn a_screen_takes_a_picture_from_the_instances_folder() {
    let dir = scratch("screen");
    let at = pinned(&dir, "de.json", false);
    let picture = dir.join("shots").join("app.png");
    fs::create_dir_all(picture.parent().expect("a folder")).expect("create");
    // A 16 by 10 picture: what an agent gives a screen slot.
    let image =
        nakkash_engine::Image::new(16, 10, [40u8, 200, 120, 255].repeat(160)).expect("an image");
    fs::write(&picture, image.to_png().expect("a PNG")).expect("write the picture");
    let mut instance = read(&at);
    instance["slots"] = json!({"screen": "shots/app.png"});
    write(&at, &instance);
    let template = digests(&dir.join("promo.json"));
    let filled = digests(&at);
    assert_eq!(filled[0], template[0], "the hook has no screen");
    assert_ne!(filled[1], template[1], "the screen shows the picture");
}

#[test]
fn an_instance_written_by_a_bare_name_pins_its_template_by_a_relative_path() {
    let dir = scratch("bare");
    let run = Command::new(env!("CARGO_BIN_EXE_nakkash"))
        .env_remove("NAKKASH_LOG")
        .current_dir(&dir)
        .args(["slots", "promo.json", "--as-instance", "--out", "fill.json"])
        .output()
        .expect("run nakkash");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        read(&dir.join("fill.json"))["template"]["path"],
        "promo.json"
    );
}
