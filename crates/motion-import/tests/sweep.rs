//! The local sweep: reads every `.aep` under `$MOTION_AEP_SAMPLES`, fails on
//! any error or panic, and prints the reports of all of them summed. Sample
//! projects whose licence forbids redistribution live there, never in the
//! repository.
//!
//! ```text
//! MOTION_AEP_SAMPLES=~/aep-samples cargo test -p motion-import -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use motion_import::{ItemKind, Project, Report, Stream};

/// Every `.aep` file under `dir`, sorted, at any depth.
fn projects(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read the samples folder")
        .map(|entry| entry.expect("read a folder entry").path())
        .flat_map(|path| match path.is_dir() {
            true => projects(&path),
            false => vec![path],
        })
        .filter(|path| path.extension().is_some_and(|e| e == "aep"))
        .collect();
    found.sort();
    found
}

/// Adds the counts of `from` to `into`.
fn add(into: &mut BTreeMap<String, usize>, from: BTreeMap<String, usize>) {
    from.into_iter()
        .for_each(|(name, count)| *into.entry(name).or_default() += count);
}

/// The properties of a project's layers, by how their values were read.
fn streams(project: &Project) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    project
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Composition(composition) => Some(composition),
            _ => None,
        })
        .flat_map(|composition| &composition.layers)
        .flat_map(|layer| layer.properties.properties())
        .for_each(|property| {
            let kind = match &property.value {
                Stream::Static(_) => "static",
                Stream::Animated(_) => "animated",
            };
            *counts.entry(kind.to_owned()).or_default() += 1;
        });
    counts
}

#[test]
#[ignore = "needs $MOTION_AEP_SAMPLES"]
fn every_sample_project_reads() {
    let dir = PathBuf::from(std::env::var("MOTION_AEP_SAMPLES").expect("set $MOTION_AEP_SAMPLES"));
    let files = projects(&dir);
    assert!(!files.is_empty(), "no `.aep` under {}", dir.display());
    let mut total = Report::default();
    let mut properties = BTreeMap::new();
    let mut failures = Vec::new();
    for path in &files {
        let bytes = fs::read(path).expect("read a sample");
        match motion_import::decode(&bytes) {
            Ok(project) => {
                println!("ok {} {} items", path.display(), project.items.len());
                add(&mut properties, streams(&project));
                let report = project.report();
                add(&mut total.items, report.items);
                add(&mut total.layers, report.layers);
                add(&mut total.effects, report.effects);
                add(&mut total.opaque, report.opaque);
                add(&mut total.skipped, report.skipped);
            }
            Err(d) => failures.push(format!("{}: {d}", path.display())),
        }
    }
    println!(
        "{} of {} projects read",
        files.len() - failures.len(),
        files.len()
    );
    println!("properties: {properties:?}\n{total}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
