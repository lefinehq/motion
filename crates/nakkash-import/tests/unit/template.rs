//! Unit tests for `src/template.rs`, on archives built in memory.

use std::io::{Cursor, Write};

use pretty_assertions::assert_eq;
use serde_json::json;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::*;

/// A zip archive of `entries`, each deflated.
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("an entry");
        writer.write_all(bytes).expect("its bytes");
    }
    writer.finish().expect("an archive").into_inner()
}

/// A `definition.json` with a text control and a group holding it.
fn definition() -> Vec<u8> {
    let name = |s: &str| json!({"strDB": [{"localeString": "en_US", "str": s}, {"localeString": "de_DE", "str": "x"}]});
    serde_json::to_vec(&json!({
        "capsuleName": "Lower third",
        "clientControls": [
            {"id": "g1", "type": 10, "uiName": name("Text"), "value": ["t1"]},
            {"id": "t1", "type": 6, "uiName": name("Name"), "value": {"strDB": [{"localeString": "en_US", "str": "Your Name"}]}},
            {"id": "s1", "type": 2, "uiName": name("Size"), "value": 100, "min": 0, "max": 400},
        ],
    }))
    .expect("JSON")
}

#[test]
fn reads_the_panel_and_the_project_inside() {
    let project = archive(&[("Lower third.aep", b"RIFX project"), ("fonts.txt", b"")]);
    let template = open(&archive(&[
        ("definition.json", &definition()),
        ("thumb.png", b"png"),
        ("project.aegraphic", &project),
    ]))
    .expect("a template");
    assert_eq!(template.project, b"RIFX project");
    let names: Vec<_> = template
        .panel
        .iter()
        .map(|c| (c.id.as_str(), c.kind, c.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("g1", ControlKind::Group, "Text"),
            ("t1", ControlKind::Text, "Name"),
            ("s1", ControlKind::Slider, "Size"),
        ]
    );
    assert_eq!(template.panel[0].value, json!(["t1"]));
    assert_eq!(
        (template.panel[2].min, template.panel[2].max),
        (Some(0.0), Some(400.0))
    );
}

#[test]
fn refuses_a_template_made_in_an_editing_app() {
    let error = open(&archive(&[
        ("definition.json", &definition()),
        ("project.prgraphic", b"other"),
    ]))
    .expect_err("no project");
    assert_eq!(error.kind, Kind::Missing);
    assert_eq!(
        error.message,
        "the template holds no project to read: it was made in an editing app"
    );
}

#[test]
fn refuses_a_project_archive_without_a_project() {
    let error = open(&archive(&[
        ("definition.json", &definition()),
        ("project.aegraphic", &archive(&[("notes.txt", b"")])),
    ]))
    .expect_err("no project");
    assert_eq!(
        error.message,
        "the template's project archive holds no project"
    );
}

#[test]
fn refuses_bytes_that_are_not_an_archive_or_have_no_panel() {
    let error = open(b"RIFX").expect_err("not a zip");
    assert!(
        error.message.starts_with("the file is not a zip archive"),
        "{}",
        error.message
    );
    let error = open(&archive(&[("thumb.png", b"png")])).expect_err("no panel");
    assert_eq!(error.message, "the template has no `definition.json`");
}

#[test]
fn names_where_the_panel_does_not_read() {
    let bad = br#"{"clientControls": [{"id": "t1", "type": "six", "uiName": {"strDB": []}}]}"#;
    let error = open(&archive(&[("definition.json", bad)])).expect_err("a bad panel");
    assert!(
        error.message.contains("at `clientControls[0].type`"),
        "{}",
        error.message
    );
}

#[test]
fn refuses_an_entry_that_inflates_past_its_limit() {
    let bytes = archive(&[("big.aep", &[0; 64])]);
    let mut zip = ZipArchive::new(Cursor::new(bytes.as_slice())).expect("an archive");
    let error = entry(&mut zip, "big.aep", 10).expect_err("too large");
    assert_eq!(
        error.message,
        "`big.aep` in the template inflates past 10 bytes"
    );
    assert_eq!(
        entry(&mut zip, "big.aep", 64)
            .expect("small enough")
            .map(|b| b.len()),
        Some(64)
    );
    assert_eq!(entry(&mut zip, "none", 64).expect("absent"), None);
}

/// Every `.mogrt` under `dir`, at any depth.
fn templates(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mogrt"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The local sweep: every sample template opens, its project decodes, and
/// every control of its panel is in the project, driving a layer that is
/// there. Prints counts only.
///
/// ```text
/// NAKKASH_AEP_SAMPLES=~/aep-samples cargo test -p nakkash-import every_sample_template -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs $NAKKASH_AEP_SAMPLES"]
fn every_sample_template_opens() {
    use crate::{ItemKind, decode};
    let dir = std::env::var("NAKKASH_AEP_SAMPLES").expect("set $NAKKASH_AEP_SAMPLES");
    let (mut files, mut controls, mut found, mut driving) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for path in templates(std::path::Path::new(&dir)) {
        let bytes = std::fs::read(&path).expect("read a sample");
        let template = match open(&bytes) {
            Ok(template) => template,
            Err(e) => {
                failures.push(format!("{}: {}", path.display(), e.message));
                continue;
            }
        };
        let project = decode(&template.project).expect("its project decodes");
        files += 1;
        let comps: Vec<_> = project
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Composition(comp) => Some((item.id, comp)),
                _ => None,
            })
            .collect();
        let panel: Vec<_> = comps.iter().flat_map(|(_, c)| &c.panel).collect();
        for control in &template.panel {
            controls += 1;
            let Some(kept) = panel.iter().find(|p| p.id == control.id) else {
                continue;
            };
            found += 1;
            let drives = kept.targets.iter().any(|t| {
                comps
                    .iter()
                    .any(|(id, c)| *id == t.composition && c.layers.iter().any(|l| l.id == t.layer))
            });
            if drives || control.kind == ControlKind::Group {
                driving += 1;
            }
        }
    }
    println!(
        "{files} templates: {controls} controls, {found} in the project, {driving} driving a layer it has (groups count)"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}
