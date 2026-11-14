//! The compatibility corpus ([D36]): scenes in `motion-format/tests/compat/`,
//! by format version, that must keep reading and drawing as they do today.
//!
//! - Each scene's first, middle and last frames are snapshots: a hash of
//!   the frame as drawn and every layer's evaluated state; a change to what a scene means shows up in
//!   review as a changed snapshot, and a breaking change bumps the version
//!   with a migration.
//! - Together the scenes use every name the format knows: every scene,
//!   composition and layer key, every layer property at rest and animated,
//!   every item kind with its properties and each choice of its settings,
//!   every text style key and every curve form. A name added to the format
//!   without a scene that uses it fails here.
//!
//! [D36]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d36-the-format-only-changes-in-the-open

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use motion_engine::{RenderOptions, Session};
use motion_format::item::{ANIMATORS, EFFECTS, ItemKind, Primary, SELECTORS, SHAPES};
use motion_format::property::PROPERTIES;
use motion_format::text::STYLE_KEYS;
use motion_format::{COMP_KEYS, LAYER_KEYS, NAMED_CURVES, SCENE_KEYS, Value, fmt_num};
use serde_json::{Map, Value as Json};

/// The corpus folder.
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../motion-format/tests/compat");

/// Every scene of the corpus, sorted, as `(v1/name, path)`.
fn scenes() -> Vec<(String, PathBuf)> {
    let mut scenes = Vec::new();
    for version in fs::read_dir(CORPUS).expect("read the corpus") {
        let version = version.expect("a corpus entry").path();
        for scene in fs::read_dir(&version).expect("read a version folder") {
            let path = scene.expect("a scene entry").path();
            if path.extension().is_some_and(|e| e == "json") {
                scenes.push((label(&version, &path), path));
            }
        }
    }
    scenes.sort();
    assert!(!scenes.is_empty(), "the corpus holds no scene");
    scenes
}

/// `v1/name` for the scene at `path` in `version`.
fn label(version: &Path, path: &Path) -> String {
    let name = |p: &Path| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .expect("a UTF-8 name")
            .to_owned()
    };
    format!("{}/{}", name(version), name(path))
}

/// FNV-1a over `bytes`: stable across platforms and Rust versions, unlike
/// the standard hasher.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn every_scene_reads_and_draws_as_it_did() {
    for (label, path) in scenes() {
        let session = Session::open(&path).unwrap_or_else(|d| panic!("{label}: {d}"));
        let last = (session.graph().expect("compile").duration_frames - 1.0).max(0.0);
        let frames: Vec<String> = [0.0, (last / 2.0).floor(), last]
            .iter()
            .map(|&frame| meaning(&session, frame).unwrap_or_else(|d| panic!("{label}: {d}")))
            .collect();
        insta::assert_snapshot!(label.replace('/', "__"), frames.join("\n"));
    }
}

/// What `session` means at `frame`, compactly: a hash of the frame as drawn,
/// then each layer's state, so a change shows which layer it moved.
fn meaning(session: &Session, frame: f64) -> motion_engine::Result<String> {
    let image = session.render_with(frame, RenderOptions::GOLDEN)?;
    let mut out = format!("frame {frame}: {:016x}\n", fnv(image.pixels()));
    for node in session.frame(frame)?.nodes {
        let [a, b, c, d, e, f] = node.transform.as_coeffs();
        let values: Vec<String> = node.values.iter().map(value).collect();
        out.push_str(&format!(
            "  {}{}{} [{}] {}\n",
            node.id,
            if node.shown { "" } else { " (not shown)" },
            if node.hidden { " (hidden)" } else { "" },
            [a, b, c, d, e, f].map(fmt_num).join(" "),
            values.join(" | "),
        ));
    }
    Ok(out)
}

/// A property value, to two decimals.
fn value(value: &Value) -> String {
    let list = |numbers: &[f64]| {
        numbers
            .iter()
            .map(|n| fmt_num(*n))
            .collect::<Vec<_>>()
            .join(",")
    };
    match value {
        Value::Number(n) => fmt_num(*n),
        Value::Point(p) => list(p),
        Value::Color(c) => list(c),
        Value::Path(path) => path.to_string(),
    }
}

#[test]
fn the_corpus_uses_every_name_the_format_knows() {
    let mut used = Used::default();
    for (label, path) in scenes() {
        let text = fs::read_to_string(&path).expect("read a scene");
        let scene: Json = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{label}: {e}"));
        used.scene(&scene);
    }
    let missing: Vec<String> = expected()
        .into_iter()
        .filter(|name| !used.names.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "the compatibility corpus uses none of: {}",
        missing.join(", ")
    );
}

/// Every name the corpus must use.
fn expected() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.extend(SCENE_KEYS.iter().map(|k| format!("scene {k}")));
    names.extend(COMP_KEYS.iter().map(|k| format!("comp {k}")));
    names.extend(LAYER_KEYS.iter().map(|k| format!("layer {k}")));
    for spec in PROPERTIES {
        names.insert(format!("layer {} at rest", spec.name));
        names.insert(format!("layer {} animated", spec.name));
    }
    for kinds in [&SHAPES[..], &EFFECTS[..], &ANIMATORS[..], &SELECTORS[..]] {
        for kind in kinds {
            names.insert(format!("item {}", kind.name));
            for row in kind.registry.rows {
                names.insert(format!("item {} {}", kind.name, row.name));
            }
            for setting in kind.settings {
                for choice in setting.choices {
                    names.insert(format!("item {} {}={choice}", kind.name, setting.name));
                }
            }
        }
    }
    names.extend(STYLE_KEYS.iter().map(|k| format!("style {}", k.name)));
    names.extend(NAMED_CURVES.iter().map(|(n, _)| format!("curve {n}")));
    names.extend(["curve cubic()", "curve speed()"].map(str::to_owned));
    names
}

/// The names a walk over scenes has met.
#[derive(Default)]
struct Used {
    /// Each as [`expected`] writes it.
    names: BTreeSet<String>,
}

impl Used {
    /// Notes `name`.
    fn note(&mut self, name: String) {
        self.names.insert(name);
    }

    /// A scene's keys, compositions and layers.
    fn scene(&mut self, scene: &Json) {
        let Some(scene) = scene.as_object() else {
            return;
        };
        scene.keys().for_each(|k| self.note(format!("scene {k}")));
        if let Some(comps) = scene.get("comps").and_then(Json::as_object) {
            for comp in comps.values().filter_map(Json::as_object) {
                comp.keys().for_each(|k| self.note(format!("comp {k}")));
                self.layers(comp.get("layers"));
            }
        }
        self.layers(scene.get("layers"));
        self.strings(&Json::Object(scene.clone()));
    }

    /// A list of layers.
    fn layers(&mut self, layers: Option<&Json>) {
        for layer in layers.and_then(Json::as_array).into_iter().flatten() {
            let Some(layer) = layer.as_object() else {
                continue;
            };
            for key in layer.keys() {
                self.note(format!("layer {key}"));
                if PROPERTIES.iter().any(|spec| spec.name == key) {
                    self.note(format!("layer {key} at rest"));
                }
            }
            self.animated(
                layer,
                |name| format!("layer {name} animated"),
                |name| {
                    PROPERTIES
                        .iter()
                        .find(|spec| spec.name == name || spec.components.contains(&name))
                        .map(|spec| spec.name)
                },
            );
            self.items(layer.get("shapes"), &SHAPES);
            self.items(layer.get("fx"), &EFFECTS);
            self.items(layer.get("animate"), &ANIMATORS);
            self.styles(layer.get("style"));
            // Runs and paragraphs write style keys directly, beside a run's range.
            for list in ["runs", "paragraphs"] {
                for entry in layer
                    .get(list)
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                {
                    self.styles(Some(entry));
                }
            }
        }
    }

    /// The tracks in `object`'s `anim`, each noted by the property it
    /// animates.
    fn animated(
        &mut self,
        object: &Map<String, Json>,
        name: impl Fn(&str) -> String,
        property: impl Fn(&str) -> Option<&'static str>,
    ) {
        for key in object
            .get("anim")
            .and_then(Json::as_object)
            .into_iter()
            .flat_map(Map::keys)
        {
            if let Some(property) = property(key) {
                self.note(name(property));
            }
        }
    }

    /// A list of items of `kinds`, and the items inside them.
    fn items(&mut self, items: Option<&Json>, kinds: &'static [ItemKind]) {
        for item in items.and_then(Json::as_array).into_iter().flatten() {
            let Some(item) = item.as_object() else {
                continue;
            };
            let Some(kind) = kinds.iter().find(|kind| item.contains_key(kind.name)) else {
                continue;
            };
            self.note(format!("item {}", kind.name));
            match (kind.primary, item.get(kind.name)) {
                (Primary::Property(property), _) => {
                    self.note(format!("item {} {property}", kind.name));
                }
                (Primary::Setting(setting), Some(Json::String(choice))) => {
                    self.note(format!("item {} {setting}={choice}", kind.name));
                }
                _ => {}
            }
            for row in kind.registry.rows {
                if item.contains_key(row.name) {
                    self.note(format!("item {} {}", kind.name, row.name));
                }
            }
            self.animated(
                item,
                |p| format!("item {} {p}", kind.name),
                |key| {
                    kind.registry
                        .rows
                        .iter()
                        .find(|row| row.name == key || row.components.contains(&key))
                        .map(|row| row.name)
                },
            );
            for setting in kind.settings {
                if let Some(choice) = item.get(setting.name).and_then(Json::as_str) {
                    self.note(format!("item {} {}={choice}", kind.name, setting.name));
                }
            }
            match kind.name {
                "group" => self.items(item.get("group"), &SHAPES),
                "animator" => self.items(item.get("animator"), &SELECTORS),
                _ => {}
            }
        }
    }

    /// A text style object's keys.
    fn styles(&mut self, style: Option<&Json>) {
        for key in style
            .and_then(Json::as_object)
            .into_iter()
            .flat_map(Map::keys)
        {
            if STYLE_KEYS.iter().any(|k| k.name == key) {
                self.note(format!("style {key}"));
            }
        }
    }

    /// The curves named in any string under `value`.
    fn strings(&mut self, value: &Json) {
        match value {
            Json::String(text) => {
                for word in text.split(|c: char| c.is_whitespace() || c == ',') {
                    if NAMED_CURVES.iter().any(|(name, _)| *name == word) {
                        self.note(format!("curve {word}"));
                    }
                    for form in ["cubic", "speed"] {
                        if word.starts_with(&format!("{form}(")) {
                            self.note(format!("curve {form}()"));
                        }
                    }
                }
            }
            Json::Array(values) => values.iter().for_each(|v| self.strings(v)),
            Json::Object(map) => map.values().for_each(|v| self.strings(v)),
            _ => {}
        }
    }
}
