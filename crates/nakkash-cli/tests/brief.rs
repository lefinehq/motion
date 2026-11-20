//! `nakkash brief` ([D91]): what an agent reads to fill a template, rules
//! and locks first, then a row per slot, in a fraction of the scene's size;
//! and a fill through it, compared with writing the scene's JSON.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const PROMO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/instance/promo.json"
);

fn nakkash(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nakkash"))
        .env_remove("NAKKASH_LOG")
        .current_dir(dir)
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
        .join("brief")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch");
    fs::copy(PROMO, dir.join("promo.json")).expect("copy the template");
    dir
}

#[test]
fn the_brief_puts_the_rules_first_and_a_row_for_every_slot() {
    let dir = scratch("text");
    let out = nakkash(&dir, &["brief", "promo.json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("# nakkash brief · promo.json · sha256:"),
        "{text}"
    );
    assert!(lines[1].starts_with("RULES "), "{text}");
    for row in [
        "| hook | hook? | 0–2 |",
        "| headline | title | Ship every market at once | ≈30 ch × 1 ln @96 (≥72: ≈40) | shrink |",
        "| cta | cta | Start free |",
        "| accent | plate | #6E56F8 | background 4:1 ≥3 |",
        "| screen | screen | empty | 16:10 | 1600x1000 | cover | image|video | product |",
        "ending ∈ {end-centred, end-left} now end-centred",
        "duration 5–8 now 6",
    ] {
        assert!(text.contains(row), "no `{row}` in\n{text}");
    }
}

#[test]
fn the_brief_as_json_has_the_same_rows() {
    let dir = scratch("json");
    let out = nakkash(&dir, &["brief", "promo.json", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let brief: Json = serde_json::from_str(&stdout(&out)).expect("JSON");
    let ids = |key: &str| -> Vec<String> {
        brief[key]
            .as_array()
            .expect(key)
            .iter()
            .map(|row| row["id"].as_str().expect("id").to_owned())
            .collect()
    };
    assert_eq!(ids("sections"), ["hook", "product", "end"]);
    assert_eq!(ids("texts"), ["headline", "feature", "cta"]);
    assert_eq!(ids("colors"), ["background", "text", "accent", "on-accent"]);
    assert_eq!(ids("media"), ["screen"]);
    assert_eq!(
        brief["texts"][0]["lines"], 1,
        "the headline's box holds one line at 96 px"
    );
    assert_eq!(brief["media"][0]["aspect"], "16:10");
}

/// Six fills an agent gets wrong, each as a slot, the key the scene
/// declares it under, and the value.
const WRONG: [(&str, &str, &str); 6] = [
    (
        "headline",
        "text",
        "Ship every single market in the world at once, today",
    ),
    ("accent", "color", "#2A2D35"),
    ("cta", "text", "Start your free trial now"),
    ("duration", "number", "12"),
    ("ending", "choice", "end-right"),
    ("screen", "image", "shot.png"),
];

/// The same six fills, once written into the scene's JSON and checked, once
/// through an instance: the brief and the instance are a fraction of the
/// scene, and no broken value reaches the instance.
#[test]
fn a_fill_through_the_brief_reads_less_and_writes_no_broken_value() {
    let dir = scratch("fill");
    let picture = nakkash_engine::Image::new(800, 600, [40u8, 120, 200, 255].repeat(800 * 600))
        .expect("an image");
    fs::write(dir.join("shot.png"), picture.to_png().expect("a PNG")).expect("write the picture");
    let scene_text = fs::read_to_string(dir.join("promo.json")).expect("read");
    let brief = stdout(&nakkash(&dir, &["brief", "promo.json"]));
    let made = nakkash(
        &dir,
        &["slots", "promo.json", "--as-instance", "--out", "fill.json"],
    );
    assert_eq!(
        made.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    let empty = fs::read_to_string(dir.join("fill.json")).expect("read");
    let read = brief.len() + empty.len();
    assert!(
        read * 10 < scene_text.len() * 6,
        "the brief and the instance are {read} bytes, the scene {}",
        scene_text.len()
    );

    let base: Json = serde_json::from_str(&scene_text).expect("JSON");
    let (mut scene_failed, mut refused, mut warned) = (0, 0, 0);
    for (slot, key, value) in WRONG {
        let mut scene = base.clone();
        scene["slots"][slot][key] = match key {
            "number" => value.parse::<f64>().expect("a number").into(),
            _ => value.into(),
        };
        fs::write(dir.join("by-hand.json"), scene.to_string()).expect("write");
        scene_failed +=
            usize::from(nakkash(&dir, &["check", "by-hand.json"]).status.code() != Some(0));

        let line = format!(
            "${slot}={}",
            if value.contains(' ') {
                format!("{value:?}")
            } else {
                value.to_owned()
            }
        );
        let out = nakkash(&dir, &["edit", "fill.json", &line]);
        let printed = stdout(&out);
        match out.status.code() {
            Some(1) => {
                refused += 1;
                assert!(printed.contains(&format!("at slots.{slot}:")), "{printed}");
                assert_eq!(
                    fs::read_to_string(dir.join("fill.json")).expect("read"),
                    empty,
                    "a refused edit writes nothing"
                );
            }
            Some(0) => {
                warned += printed
                    .lines()
                    .filter(|l| l.starts_with("warning "))
                    .count();
                fs::write(dir.join("fill.json"), &empty).expect("reset");
            }
            other => panic!("edit `{line}` exited {other:?}: {printed}"),
        }
    }
    eprintln!(
        "read: scene {} bytes, brief + instance {read}; of {} wrong fills, check on the scene failed {scene_failed}, the instance refused {refused} and warned {warned} times on the rest",
        scene_text.len(),
        WRONG.len()
    );
    assert_eq!(refused, 5, "every fill but the small picture is refused");
    assert_eq!(
        warned, 2,
        "the small picture is too small and the wrong shape"
    );
    assert!(scene_failed < refused);
}
