//! The whole chain in CI (hardening plan H3): every fixture project the
//! repository holds, imported and drawn at its first, middle and last
//! frames, held to a snapshot of what it meant. The local samples measure
//! fidelity; these catch a change anywhere from the decoder to the pixels.
//!
//! A fixture whose scene cannot be drawn here, such as text in a font the
//! repository does not hold, snapshots why, so that changes too.

use std::fs;
use std::path::{Path, PathBuf};

use motion_engine::{ImportOptions, Session, import_scene, write_document};

mod common;

/// The fixture folder.
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../motion-import/tests/fixtures"
);

#[test]
fn every_fixture_draws_as_it_did() {
    let mut paths: Vec<PathBuf> = fs::read_dir(FIXTURES)
        .expect("read the fixture folder")
        .map(|entry| entry.expect("a folder entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "aep"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 10, "only {} fixtures", paths.len());
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("a UTF-8 name");
        let meaning = match import_scene(&path, &first_composition(&path), None) {
            Err(error) => format!("not imported: {error}"),
            Ok(import) => match write_document(&import.json).parse::<Session>() {
                Err(error) => format!("not read: {error}"),
                Ok(session) => {
                    common::meaning(&session).unwrap_or_else(|error| format!("not drawn: {error}"))
                }
            },
        };
        insta::assert_snapshot!(name, meaning);
    }
}

/// Options importing the project's first composition, as the import's own
/// snapshot tests do: a fixture may have several that no other draws.
fn first_composition(path: &Path) -> ImportOptions {
    let project = motion_engine::import(path).expect("the fixture decodes");
    let comp = project.items.iter().find_map(|item| match item.kind {
        motion_import::ItemKind::Composition(_) => Some(item.name.clone()),
        _ => None,
    });
    ImportOptions {
        comp,
        fonts: Vec::new(),
        ..Default::default()
    }
}
