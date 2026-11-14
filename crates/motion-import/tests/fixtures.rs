//! Tests on the committed fixture projects (sources in `fixtures/NOTICE.md`):
//! every one reads, prints as its snapshot, and survives truncation and
//! corruption without a panic; and the reader agrees with the Lottie export
//! of each project that has one, an independent reading of the same file.

use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};

use motion_import::{Child, Composition, Group, ItemKind, Layer, Project, Stream, Value, decode};
use proptest::prelude::*;
use serde_json::Value as Json;

/// The fixture folder.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// Lottie's transform keys: the match name each one is, and the factor from
/// the file's units to Lottie's (scale and opacity are fractions in the file).
const TRANSFORM: [(&str, &str, f64); 5] = [
    ("a", "ADBE Anchor Point", 1.0),
    ("p", "ADBE Position", 1.0),
    ("s", "ADBE Scale", 100.0),
    ("r", "ADBE Rotate Z", 1.0),
    ("o", "ADBE Opacity", 100.0),
];

/// The Lottie keys of a separated position, and the match names they are.
const SEPARATED: [(&str, &str); 2] = [("x", "ADBE Position_0"), ("y", "ADBE Position_1")];

/// How far two times in frames, or two values, may differ.
const TOLERANCE: f64 = 1e-3;

/// Where the fixtures and their Lottie exports are known to disagree, and why.
///
/// The `Masks` export was made from another revision of the project: in it the
/// layer's position moves over 60 frames, while the committed project holds a
/// static position and no keyframe list for it at all.
const KNOWN: [&str; 1] = ["Masks: layer \"Shape Layer 1\": ADBE Position: 0 keyframes, Lottie 2"];

/// The files in `dir` with `extension`, sorted.
fn files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read the fixture folder")
        .map(|entry| entry.expect("read a folder entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .collect();
    found.sort();
    found
}

/// The file name of `path` without its extension.
fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .expect("a UTF-8 file name")
        .to_owned()
}

/// The fixture project at `path`, read.
fn project(path: &Path) -> Project {
    let bytes = fs::read(path).expect("read a fixture");
    decode(&bytes).unwrap_or_else(|d| panic!("{}: {d}", path.display()))
}

#[test]
fn every_fixture_prints_as_its_snapshot() {
    let fixtures = files(Path::new(FIXTURES), "aep");
    assert!(fixtures.len() >= 10, "{fixtures:?}");
    for path in fixtures {
        let project = project(&path);
        insta::assert_snapshot!(stem(&path), format!("{project}{}", project.report()));
    }
}

/// The composition the Lottie export `lottie` was made from: the one with its
/// name, or, for exports without one, the one with its size.
fn exported<'p>(project: &'p Project, lottie: &Json) -> &'p Composition {
    let compositions = project.items.iter().filter_map(|item| match &item.kind {
        ItemKind::Composition(composition) => Some((item.name.as_str(), composition)),
        _ => None,
    });
    let matches = |(name, c): &(&str, &Composition)| match lottie["nm"].as_str() {
        Some(exported) => *name == exported,
        None => {
            Some(u64::from(c.width)) == lottie["w"].as_u64()
                && Some(u64::from(c.height)) == lottie["h"].as_u64()
        }
    };
    compositions
        .clone()
        .find(matches)
        .map(|(_, composition)| composition)
        .expect("the exported composition")
}

/// The number `json` holds.
fn number(json: &Json) -> f64 {
    json.as_f64()
        .unwrap_or_else(|| panic!("a number, not {json}"))
}

/// The property called `match_name` in the transform group of `layer`.
fn transform_stream<'l>(layer: &'l Layer, match_name: &str) -> Option<&'l Stream> {
    let transform = layer
        .properties
        .children
        .iter()
        .find_map(|child| match child {
            Child::Group(group) if group.match_name == "ADBE Transform Group" => Some(group),
            _ => None,
        })?;
    find_property(transform, match_name)
}

/// The value of the property called `match_name` directly in `group`.
fn find_property<'g>(group: &'g Group, match_name: &str) -> Option<&'g Stream> {
    group.children.iter().find_map(|child| match child {
        Child::Property(property) if property.match_name == match_name => Some(&property.value),
        _ => None,
    })
}

/// The numbers of a value, as Lottie lists them.
fn numbers(value: &Value) -> Vec<f64> {
    match value {
        Value::Number(x) => vec![*x],
        Value::Vector2(v) => v.to_vec(),
        Value::Vector3(v) => v.to_vec(),
        other => panic!("a number or a vector, not {other:?}"),
    }
}

/// The keyframes of a Lottie property as times in frames and start values,
/// none when it is static. An old export's last keyframe has no start value;
/// it is the previous keyframe's end value.
fn lottie_keyframes(property: &Json) -> Vec<(f64, Vec<f64>)> {
    let Some(keys) = property["k"].as_array() else {
        return Vec::new();
    };
    if keys.first().and_then(|key| key.get("t")).is_none() {
        return Vec::new();
    }
    let values = |json: &Json| match json {
        Json::Array(items) => items.iter().map(number).collect(),
        other => vec![number(other)],
    };
    let mut previous_end = Vec::new();
    keys.iter()
        .map(|key| {
            let start = key.get("s").map_or_else(|| previous_end.clone(), values);
            previous_end = key.get("e").map_or_else(|| start.clone(), values);
            (number(&key["t"]), start)
        })
        .collect()
}

/// Where the reader and a Lottie export disagree, one line each.
#[derive(Default)]
struct Disagreements(Vec<String>);

impl Disagreements {
    /// Notes `what` when `ours` and `lottie` differ by more than [`TOLERANCE`].
    fn close(&mut self, what: &str, ours: f64, lottie: f64) {
        if (ours - lottie).abs() > TOLERANCE {
            self.0.push(format!("{what}: ours {ours}, Lottie {lottie}"));
        }
    }

    /// Notes `what` when `ours` and `lottie` differ.
    fn equal<T: PartialEq + Debug>(&mut self, what: &str, ours: T, lottie: T) {
        if ours != lottie {
            self.0
                .push(format!("{what}: ours {ours:?}, Lottie {lottie:?}"));
        }
    }

    /// Notes where our stream and the Lottie property hold different keyframes.
    fn keyframes(
        &mut self,
        what: &str,
        ours: Option<&Stream>,
        lottie: &Json,
        factor: f64,
        fps: f64,
    ) {
        let keys = match ours {
            Some(Stream::Animated(keys)) => keys.as_slice(),
            _ => &[],
        };
        let expected = lottie_keyframes(lottie);
        if keys.len() != expected.len() {
            let (ours, lottie) = (keys.len(), expected.len());
            self.0
                .push(format!("{what}: {ours} keyframes, Lottie {lottie}"));
            return;
        }
        for (index, (key, (time, values))) in keys.iter().zip(&expected).enumerate() {
            self.close(&format!("{what}[{index}] time"), key.time * fps, *time);
            for (dim, (ours, lottie)) in numbers(&key.value).iter().zip(values).enumerate() {
                self.close(&format!("{what}[{index}][{dim}]"), ours * factor, *lottie);
            }
        }
    }

    /// Notes where a composition and its export disagree: size, rate, work
    /// area, and each layer's name, timing, parent and transform keyframes.
    fn composition(&mut self, name: &str, comp: &Composition, lottie: &Json) {
        let fps = comp.frame_rate;
        let size = |n: u16| Some(u64::from(n));
        self.equal(
            &format!("{name}: width"),
            size(comp.width),
            lottie["w"].as_u64(),
        );
        self.equal(
            &format!("{name}: height"),
            size(comp.height),
            lottie["h"].as_u64(),
        );
        self.close(&format!("{name}: frame rate"), fps, number(&lottie["fr"]));
        let work_area = comp.work_area;
        self.close(
            &format!("{name}: in point"),
            work_area.start * fps,
            number(&lottie["ip"]),
        );
        self.close(
            &format!("{name}: out point"),
            work_area.end * fps,
            number(&lottie["op"]),
        );
        let layers = lottie["layers"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        self.equal(&format!("{name}: layers"), comp.layers.len(), layers.len());
        let index_of_ind = |ind: &Json| layers.iter().position(|l| l["ind"] == *ind);
        for (layer, exported) in comp.layers.iter().zip(layers) {
            let what = format!("{name}: layer {:?}", layer.name);
            let exported_name = exported["nm"].as_str();
            self.equal(
                &format!("{what}: name"),
                Some(layer.name.as_str()),
                exported_name,
            );
            self.close(
                &format!("{what}: in point"),
                layer.in_point * fps,
                number(&exported["ip"]),
            );
            self.close(
                &format!("{what}: out point"),
                layer.out_point * fps,
                number(&exported["op"]),
            );
            let parent = layer
                .parent
                .and_then(|id| comp.layers.iter().position(|l| l.id == id));
            let exported_parent = exported.get("parent").and_then(index_of_ind);
            self.equal(&format!("{what}: parent"), parent, exported_parent);
            let transform = &exported["ks"];
            for (key, match_name, factor) in TRANSFORM {
                let property = &transform[key];
                let streams: Vec<(&str, &Json)> = match property.get("s").and_then(Json::as_bool) {
                    Some(true) => SEPARATED
                        .iter()
                        .map(|(axis, name)| (*name, &property[*axis]))
                        .collect(),
                    _ => vec![(match_name, property)],
                };
                for (match_name, exported) in streams {
                    let ours = transform_stream(layer, match_name);
                    self.keyframes(
                        &format!("{what}: {match_name}"),
                        ours,
                        exported,
                        factor,
                        fps,
                    );
                }
            }
        }
    }
}

#[test]
fn agrees_with_the_lottie_exports() {
    let exports = files(&Path::new(FIXTURES).join("lottie"), "json");
    assert!(exports.len() >= 8, "{exports:?}");
    let mut disagreements = Disagreements::default();
    for export in exports {
        let name = stem(&export);
        let lottie: Json = serde_json::from_slice(&fs::read(&export).expect("read an export"))
            .expect("an export is JSON");
        let project = project(&Path::new(FIXTURES).join(format!("{name}.aep")));
        disagreements.composition(&name, exported(&project, &lottie), &lottie);
    }
    assert_eq!(disagreements.0, KNOWN);
}

/// A small fixture with layers, shapes and keyframes, for the corruption tests.
fn star() -> Vec<u8> {
    fs::read(Path::new(FIXTURES).join("Star.aep")).expect("read the star fixture")
}

proptest! {
    #[test]
    fn never_panics_on_a_prefix_of_a_fixture(fraction in 0.0f64..1.0) {
        let bytes = star();
        let end = (bytes.len() as f64 * fraction) as usize;
        let _ = decode(bytes.get(..end).unwrap_or_default());
    }

    #[test]
    fn never_panics_on_a_corrupted_fixture(
        changes in proptest::collection::vec((0.0f64..1.0, any::<u8>()), 1..16)
    ) {
        let mut bytes = star();
        let len = bytes.len() as f64;
        changes.iter().for_each(|(at, byte)| {
            if let Some(b) = bytes.get_mut((len * at) as usize) {
                *b = *byte;
            }
        });
        let _ = decode(&bytes);
    }
}
