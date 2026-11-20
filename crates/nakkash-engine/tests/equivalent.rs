//! Two forms of one scene are one scene: every way of writing a scene that
//! the format offers as the same (a scene written out pretty or compact, its
//! styles and eases written once, or each composition in a file of its
//! own) must
//! read into the same validated [`Scene`](nakkash_format::Scene) and draw the
//! same frames, to the bit. Key order is not one of them: a layer keeps its
//! resting values in the order they are written. Every scene of the compatibility corpus, the
//! examples and the fixtures is written in each form, read back and drawn.
//!
//! `NAKKASH_EQUIVALENT_SCENES`, folders of scene files joined as `PATH` joins
//! folders, adds every scene in them at any depth, such as large imported
//! templates, which cannot ship here; `NAKKASH_EQUIVALENT_FRAMES` sets how
//! many frames of each scene are drawn, spread from the first to the last
//! (`all` draws every one, a release-mode job for long scenes).
//!
//! A form is written beside the scene it comes from, under a name starting
//! with a dot, so the files a scene names resolve as they do for the scene,
//! and other tests listing the same folders leave it out. It is removed when
//! the comparison ends, failed or not.

use std::fs;
use std::path::{Path, PathBuf};

use nakkash_engine::{RenderOptions, Session};
use serde_json::Value as Json;

/// Frames drawn per scene when `NAKKASH_EQUIVALENT_FRAMES` does not say:
/// few, so the test stays quick unoptimised.
const FRAMES: usize = 3;

/// Every scene file the test reads, sorted.
fn scenes() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut scenes = Vec::new();
    for folder in [
        "crates/nakkash-format/tests/compat/v1",
        "examples",
        "crates/nakkash-engine/tests/fixtures",
    ] {
        scenes_in(&root.join(folder), false, &mut scenes);
    }
    if let Some(extra) = std::env::var_os("NAKKASH_EQUIVALENT_SCENES") {
        for folder in std::env::split_paths(&extra) {
            scenes_in(&folder, true, &mut scenes);
        }
    }
    scenes.sort();
    scenes
}

/// Adds the scene files in `folder`, and with `deep` in the folders inside
/// it, to `scenes`; files whose names start with a dot are left out.
fn scenes_in(folder: &Path, deep: bool, scenes: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a folder entry").path();
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() && deep {
            scenes_in(&path, deep, scenes);
        } else if path.extension().is_some_and(|e| e == "json") && is_scene(&path) {
            scenes.push(path);
        }
    }
}

/// Whether the JSON file at `path` is a scene, by its version key; a
/// folder may hold recipes, reports and locks beside its scenes.
fn is_scene(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Json>(&text).ok())
        .is_some_and(|json| json.get("nakkash").is_some() || json.get("motion").is_some())
}

/// One way of writing a scene that must mean the same scene.
struct Form {
    /// Its name, in failure messages and the written file's name.
    name: &'static str,
    /// The text of the scene's JSON in this form.
    write: fn(&Json) -> String,
    /// Whether the form keeps each composition in a file of its own (D91).
    split: bool,
}

/// The forms every scene is written in.
const FORMS: &[Form] = &[
    Form {
        name: "pretty",
        write: |json| serde_json::to_string_pretty(json).expect("JSON writes"),
        split: false,
    },
    Form {
        name: "compact",
        write: |json| serde_json::to_string(json).expect("JSON writes"),
        split: false,
    },
    // Repeated effect stacks and animations as styles, repeated curves as
    // eases (D91); defaults are left to `nakkash compact`, which proves
    // each one, since a layer keeps the resting values written on it.
    Form {
        name: "styled",
        write: |json| {
            let parts = nakkash_engine::compact::Parts {
                defaults: false,
                styles: true,
                eases: true,
            };
            let styled = nakkash_engine::compact::compact(json, parts)
                .map_or_else(|_| json.clone(), |(styled, _)| styled);
            serde_json::to_string(&styled).expect("JSON writes")
        },
        split: false,
    },
    // Each composition in a file of its own, read back through its include.
    Form {
        name: "split",
        write: |json| serde_json::to_string(json).expect("JSON writes"),
        split: true,
    },
];

/// A file, and the folder of its compositions' files if it has one, which
/// are removed when dropped.
struct Written(PathBuf, Option<PathBuf>);

impl Drop for Written {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        if let Some(folder) = &self.1 {
            let _ = fs::remove_dir_all(folder);
        }
    }
}

/// `scene` written in `form` beside it.
fn write(scene: &Path, json: &Json, form: &Form) -> Written {
    let stem = scene.file_stem().expect("a file name").to_string_lossy();
    let name = format!(".{stem}.{}.{}.json", form.name, std::process::id());
    let path = scene.with_file_name(name);
    if !form.split {
        fs::write(&path, (form.write)(json)).expect("write the form");
        return Written(path, None);
    }
    // The compositions' files go in a folder of this form's own, so forms of
    // other scenes in the same folder, and other runs, keep apart.
    let folder = format!(".{stem}.{}.{}.comps", form.name, std::process::id());
    let (mut top, comps) = nakkash_format::include::split(json);
    fs::create_dir_all(scene.with_file_name(&folder)).expect("make the folder");
    for (file, comp) in comps {
        let name = file.strip_prefix("comps/").expect("a composition's file");
        let id = name.strip_suffix(".json").expect("a JSON file");
        fs::write(scene.with_file_name(&folder).join(name), comp.to_string())
            .expect("write a composition");
        top["comps"][id]["include"] = Json::String(format!("{folder}/{name}"));
    }
    fs::write(&path, (form.write)(&top)).expect("write the form");
    Written(path, Some(scene.with_file_name(folder)))
}

/// The frames of `session` to draw: `n` spread from the first to the last,
/// or all of them.
fn frames(session: &Session) -> Vec<u32> {
    let Ok(all) = session.frames() else {
        return Vec::new();
    };
    let n = match std::env::var("NAKKASH_EQUIVALENT_FRAMES").as_deref() {
        Ok("all") => usize::MAX,
        Ok(n) => n
            .parse()
            .expect("NAKKASH_EQUIVALENT_FRAMES is a number or `all`"),
        Err(_) => FRAMES,
    };
    let len = all.len();
    if len == 0 || n == 0 {
        return Vec::new();
    }
    if n >= len {
        return all.collect();
    }
    let mut picked: Vec<u32> = (0..n)
        .map(|i| all.start + (i * (len - 1) / (n - 1).max(1)) as u32)
        .collect();
    picked.dedup();
    picked
}

/// What a scene file reads and draws as, for comparing: the validated
/// scene, then each frame's pixels, or the first error, as text.
fn reading(path: &Path, at: &[u32]) -> (String, Vec<Result<Vec<u8>, String>>) {
    let session = match Session::open(path) {
        Ok(session) => session,
        Err(e) => return (format!("error: {e}"), Vec::new()),
    };
    let scene = format!("{:?}", session.scene());
    let drawn = at
        .iter()
        .map(|&frame| {
            session
                .render_with(f64::from(frame), RenderOptions::default())
                .map(|image| image.pixels().to_vec())
                .map_err(|e| e.to_string())
        })
        .collect();
    (scene, drawn)
}

/// The stack the comparison runs on: reading and drawing a deeply nested
/// scene goes deeper than a test thread's default stack, as the drawing
/// pool's threads do.
const STACK: usize = 64 << 20;

#[test]
fn every_form_of_a_scene_reads_and_draws_as_the_scene() {
    std::thread::Builder::new()
        .name("equivalent".into())
        .stack_size(STACK)
        .spawn(compare_every_form)
        .expect("start the comparison")
        .join()
        .expect("the comparison ends");
}

fn compare_every_form() {
    let scenes = scenes();
    assert!(!scenes.is_empty(), "no scene to compare");
    let mut failures = Vec::new();
    for scene in &scenes {
        eprintln!("comparing {}", scene.display());
        let text = fs::read_to_string(scene).expect("read the scene");
        let json: Json = serde_json::from_str(&text).expect("the scene is JSON");
        let at = Session::open(scene).map(|s| frames(&s)).unwrap_or_default();
        let (want_scene, want_frames) = reading(scene, &at);
        for form in FORMS {
            let written = write(scene, &json, form);
            let (scene_read, frames_drawn) = reading(&written.0, &at);
            let name = scene.display();
            if scene_read != want_scene {
                failures.push(format!("{name} as {}: reads as another scene", form.name));
                continue;
            }
            for ((frame, want), got) in at.iter().zip(&want_frames).zip(&frames_drawn) {
                if want != got {
                    failures.push(format!("{name} as {}: frame {frame} differs", form.name));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
