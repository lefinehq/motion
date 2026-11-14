//! Imports of the committed fixture projects: every composition of every
//! fixture imports as a scene the format accepts, the scene reads back
//! unchanged once written, and each fixture's scene and report print as
//! their snapshot.

use std::fs;
use std::path::{Path, PathBuf};

use motion_format::{parse_document, write_document};
use motion_import::map::{Options, import};
use motion_import::{ItemKind, Project, decode};

/// The fixture folder.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// The fixture projects, sorted, each read.
fn fixtures() -> Vec<(String, Project)> {
    let mut paths: Vec<PathBuf> = fs::read_dir(FIXTURES)
        .expect("read the fixture folder")
        .map(|entry| entry.expect("read a folder entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "aep"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let bytes = fs::read(path).expect("read a fixture");
            let project = decode(&bytes).unwrap_or_else(|d| panic!("{}: {d}", path.display()));
            (stem(path), project)
        })
        .collect()
}

/// The file name of `path` without its extension.
fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .expect("a UTF-8 file name")
        .to_owned()
}

/// The names of `project`'s compositions, in project order.
fn compositions(project: &Project) -> Vec<String> {
    project
        .items
        .iter()
        .filter(|item| matches!(item.kind, ItemKind::Composition(_)))
        .map(|item| item.name.clone())
        .collect()
}

#[test]
fn every_composition_imports_and_reads_back_unchanged() {
    let mut imported = 0;
    for (name, project) in fixtures() {
        for comp in compositions(&project) {
            let options = Options {
                comp: Some(comp.clone()),
                fonts: Vec::new(),
            };
            let import =
                import(&project, &options).unwrap_or_else(|d| panic!("{name}/{comp}: {d}"));
            let written = write_document(&import.json);
            let read = parse_document(&written).unwrap_or_else(|d| panic!("{name}/{comp}: {d}"));
            assert_eq!(read, import.document, "{name}/{comp}");
            imported += 1;
        }
    }
    assert!(imported >= 20, "only {imported} compositions imported");
}

#[test]
fn every_fixture_imports_as_its_snapshot() {
    for (name, project) in fixtures() {
        let Some(first) = compositions(&project).into_iter().next() else {
            continue;
        };
        let options = Options {
            comp: Some(first),
            fonts: Vec::new(),
        };
        let import = import(&project, &options).expect("the composition imports");
        let scene = write_document(&import.json);
        insta::assert_snapshot!(name, format!("{scene}\n{}", import.mapping));
    }
}
