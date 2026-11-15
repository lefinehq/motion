//! Unit tests for `src/export.rs`: edits written back into real projects.

use std::fs;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;
use crate::decode;
use crate::map::{Import, Options, import};

/// The fixture projects that import as a scene, with their bytes.
fn fixtures() -> Vec<(String, Vec<u8>, Import)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .expect("the fixtures")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "aep"))
        .collect();
    found.sort();
    found
        .into_iter()
        .filter_map(|path| {
            let bytes = fs::read(&path).expect("read a fixture");
            let project = decode(&bytes).ok()?;
            let imported = import(&project, &Options::default()).ok()?;
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some((name, bytes, imported))
        })
        .collect()
}

/// The scene imported from `bytes`.
fn reimport(bytes: &[u8]) -> Json {
    import(&decode(bytes).expect("decodes"), &Options::default())
        .expect("imports")
        .json
}

/// `json` with the value at `path` set to `value`.
fn set(json: &mut Json, path: &[Step], value: Json) {
    let mut at = json;
    for step in path {
        at = match step {
            Step::Key(key) => &mut at[key.as_str()],
            Step::Index(i) => &mut at[*i],
            Step::Id(id) => at
                .as_array_mut()
                .expect("an array")
                .iter_mut()
                .find(|item| item["id"] == id.as_ref())
                .expect("the id"),
        };
    }
    *at = value;
}

/// A different value of the same shape.
fn nudged(value: &Json) -> Json {
    match value {
        Json::Number(n) => json!(n.as_f64().unwrap_or(0.0) * 0.5 + 0.1),
        Json::Array(items) => Json::Array(items.iter().map(nudged).collect()),
        other => other.clone(),
    }
}

#[test]
fn every_fixture_writes_back_unchanged() {
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= 5,
        "only {} fixtures import",
        fixtures.len()
    );
    for (name, bytes, imported) in &fixtures {
        assert_eq!(rifx::write(bytes, &[]).unwrap(), *bytes, "{name}");
        let plan = plan(&imported.json, &imported.sources, &imported.json);
        assert_eq!(plan, Plan::default(), "{name}");
        assert_eq!(apply(bytes, &plan).unwrap(), *bytes, "{name}");
    }
}

#[test]
fn every_static_value_of_every_fixture_writes_back() {
    let mut written = 0;
    for (name, bytes, imported) in fixtures() {
        let statics: Vec<&Source> = imported
            .sources
            .0
            .iter()
            .filter(|s| matches!(s.writable, Writable::Static { .. }))
            .collect();
        for source in statics {
            let mut edited = imported.json.clone();
            let before = json_at(&edited, &source.path).expect("marked").clone();
            set(&mut edited, &source.path, nudged(&before));
            let plan = plan(&imported.json, &imported.sources, &edited);
            assert_eq!(
                plan.written.len(),
                1,
                "{name} {}: {plan}",
                key_text(&source.path)
            );
            assert!(plan.not_written.is_empty(), "{name}: {plan}");
            let patched = apply(&bytes, &plan).expect("patches");
            assert_eq!(patched.len(), bytes.len(), "a static value keeps its size");
            let back = reimport(&patched);
            assert_eq!(
                verify(&plan, &back),
                [],
                "{name} {}",
                key_text(&source.path)
            );
            written += 1;
        }
    }
    assert!(written >= 20, "only {written} values written");
}

#[test]
fn edits_without_a_stored_value_are_reported() {
    let (name, _, imported) = fixtures()
        .into_iter()
        .find(|(_, _, i)| i.json["layers"].as_array().is_some_and(|l| !l.is_empty()))
        .expect("a fixture with layers");
    let mut edited = imported.json.clone();
    edited["layers"][0]["note"] = json!("new");
    edited["bg"] = json!("#123456");
    let plan = plan(&imported.json, &imported.sources, &edited);
    assert!(plan.written.is_empty(), "{name}: {plan}");
    let whys: Vec<(&str, &str)> = plan
        .not_written
        .iter()
        .map(|c| (c.at.as_str(), c.why.as_deref().unwrap_or_default()))
        .collect();
    assert!(
        whys.iter()
            .any(|(at, why)| *at == "bg" && why.contains("stores no value")),
        "{whys:?}"
    );
    assert!(
        whys.iter()
            .any(|(at, why)| at.ends_with(".note") && why.contains("no such")),
        "{whys:?}"
    );
}

#[test]
fn a_colour_goes_back_as_alpha_red_green_blue_out_of_255() {
    let origin = Origin {
        numbers: 0..32,
        dims: 4,
        color: true,
    };
    let template = Value::Color(Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let value = project_value(&json!("#FF8000"), Convert::Color, &template).unwrap();
    let bytes = encode(&value, &origin).unwrap();
    let numbers: Vec<f64> = bytes
        .chunks(8)
        .map(|c| f64::from_be_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(numbers, [255.0, 255.0, 128.0, 0.0]);
}

#[test]
fn a_value_on_a_half_step_may_read_back_one_step_away() {
    // 0.1177975 rounds to 0.117798, and through the file, out of 255, it
    // comes back a hair under the half step, which rounds to 0.117797.
    let through_the_file = 0.117_797_5_f64 * 255.0 / 255.0;
    assert!(through_the_file < 0.117_797_5);
    assert!(close(&json!(0.117_797), &json!(0.117_798)));
    assert!(close(&json!(1_920.000_001), &json!(1_920.000_003)));
    assert!(!close(&json!(0.117_797), &json!(0.117_799)), "two steps");
    assert!(!close(&json!([0.5, 0.25]), &json!([0.5, 0.2501])));
}

#[test]
fn a_point_keeps_the_projects_depth_and_a_percent_scales_back() {
    let template = Value::Vector3([1.0, 2.0, 7.0]);
    assert_eq!(
        project_value(&json!([10, 20]), Convert::Point, &template),
        Ok(Value::Vector3([10.0, 20.0, 7.0]))
    );
    assert_eq!(
        project_value(&json!(0.5), Convert::Percent, &Value::Number(100.0)),
        Ok(Value::Number(50.0))
    );
    assert_eq!(
        project_value(&json!(1.5), Convert::Point, &Value::Vector2([1.0, 1.0])),
        Ok(Value::Vector2([1.5, 1.5])),
        "one number sets both, as the scene reads it"
    );
}

/// Every `.aep` under `dir`, at any depth.
fn projects(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read a folder")
        .map(|e| e.expect("an entry").path())
        .flat_map(|path| match path.is_dir() {
            true => projects(&path),
            false => vec![path],
        })
        .filter(|path| path.extension().is_some_and(|e| e == "aep"))
        .collect();
    found.sort();
    found
}

/// The local sweep: every sample project writes back byte for byte, and
/// every static value of its scene, edited all at once, reads back as
/// edited.
///
/// ```text
/// MOTION_AEP_SAMPLES=~/aep-samples cargo test -p motion-import every_sample -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs $MOTION_AEP_SAMPLES"]
fn every_sample_project_writes_back() {
    let dir = PathBuf::from(std::env::var("MOTION_AEP_SAMPLES").expect("set $MOTION_AEP_SAMPLES"));
    let (mut files, mut identical, mut scenes, mut values, mut failures) = (0, 0, 0, 0, Vec::new());
    for path in projects(&dir) {
        let started = std::time::Instant::now();
        let bytes = fs::read(&path).expect("read a sample");
        let Ok(project) = decode(&bytes) else {
            continue;
        };
        files += 1;
        if apply(&bytes, &Plan::default()).as_deref() == Ok(bytes.as_slice()) {
            identical += 1;
        } else {
            failures.push(format!("{}: not identical", path.display()));
        }
        let comps = project.items.iter().filter_map(|item| match item.kind {
            crate::ItemKind::Composition(_) => Some(item.name.clone()),
            _ => None,
        });
        let Some(imported) = std::iter::once(None)
            .chain(comps.map(Some))
            .find_map(|comp| {
                import(
                    &project,
                    &Options {
                        comp,
                        ..Options::default()
                    },
                )
                .ok()
                .map(|i| (i, ()))
            })
            .map(|(i, ())| i)
        else {
            continue;
        };
        scenes += 1;
        let mut edited = imported.json.clone();
        let statics: Vec<&Source> = imported
            .sources
            .0
            .iter()
            .filter(|s| matches!(s.writable, Writable::Static { .. }))
            .collect();
        for source in &statics {
            let before = json_at(&edited, &source.path).expect("marked").clone();
            set(&mut edited, &source.path, nudged(&before));
        }
        let plan = plan(&imported.json, &imported.sources, &edited);
        values += plan.written.len();
        let patched = match apply(&bytes, &plan) {
            Ok(patched) => patched,
            Err(e) => {
                failures.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let options = Options {
            comp: None,
            ..Options::default()
        };
        let back = decode(&patched).and_then(|p| {
            import(&p, &options).or_else(|_| {
                let comp = imported.mapping.scene.clone();
                import(
                    &p,
                    &Options {
                        comp: Some(comp),
                        ..Options::default()
                    },
                )
            })
        });
        match back {
            Ok(back) => {
                let wrong = verify(&plan, &back.json);
                if !wrong.is_empty() {
                    failures.push(format!(
                        "{}: {} of {} wrong, first {:?}",
                        path.display(),
                        wrong.len(),
                        plan.written.len(),
                        wrong[0]
                    ));
                }
            }
            Err(e) => failures.push(format!("{}: patched does not import: {e}", path.display())),
        }
        println!(
            "{:>6.1}s {} ({} written)",
            started.elapsed().as_secs_f64(),
            path.display(),
            plan.written.len()
        );
    }
    println!(
        "{files} projects: {identical} write back identical; {scenes} scenes, {values} static values written back and read back"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
