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

use motion_import::map::{Options, import};
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

#[test]
#[ignore = "needs $MOTION_AEP_SAMPLES"]
fn every_sample_composition_imports() {
    let dir = PathBuf::from(std::env::var("MOTION_AEP_SAMPLES").expect("set $MOTION_AEP_SAMPLES"));
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    let mut fates = [0usize; 4];
    let mut failures = Vec::new();
    let mut imported = 0;
    for path in projects(&dir) {
        let bytes = fs::read(&path).expect("read a sample");
        let Ok(project) = motion_import::decode(&bytes) else {
            continue;
        };
        let names = project.items.iter().filter_map(|item| match item.kind {
            ItemKind::Composition(_) => Some(item.name.clone()),
            _ => None,
        });
        for comp in names {
            let options = Options {
                comp: Some(comp.clone()),
                fonts: Vec::new(),
                ..Default::default()
            };
            match import(&project, &options) {
                Ok(import) => {
                    imported += 1;
                    let mapping = import.mapping;
                    mapping.counts.values().for_each(|c| {
                        fates[0] += c.mapped;
                        fates[1] += c.defaulted;
                        fates[2] += c.ignored;
                        fates[3] += c.unsupported;
                    });
                    mapping.unsupported.iter().for_each(|finding| {
                        *unsupported.entry(finding.what.clone()).or_default() += 1;
                    });
                }
                Err(d) => failures.push(format!("{}/{comp}: {d}", path.display())),
            }
        }
    }
    let [mapped, defaulted, ignored, missing] = fates;
    println!(
        "{imported} compositions imported: {mapped} mapped, {defaulted} defaulted, \
         {ignored} ignored, {missing} unsupported"
    );
    let mut ranked: Vec<(&String, &usize)> = unsupported.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    ranked
        .iter()
        .for_each(|(what, count)| println!("  {count:5} {what}"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every text document in `group` and below: a static one, and each key's.
fn documents<'p>(group: &'p motion_import::Group, out: &mut Vec<&'p motion_import::TextDocument>) {
    for child in &group.children {
        match child {
            motion_import::Child::Group(group) => documents(group, out),
            motion_import::Child::Property(property) => match &property.value {
                Stream::Static(motion_import::Value::Text(document)) => out.push(document),
                Stream::Animated(keys) => {
                    out.extend(keys.iter().filter_map(|key| match &key.value {
                        motion_import::Value::Text(document) => Some(&**document),
                        _ => None,
                    }))
                }
                Stream::Static(_) => {}
            },
        }
    }
}

/// The beliefs about the composed-line cache that the text check rests on
/// (`docs/design/aep-evidence.md`), each held over every stored layout.
#[test]
#[ignore = "needs $MOTION_AEP_SAMPLES"]
fn the_composed_line_cache_holds_what_we_believe() {
    use motion_import::{Justification, TextFrame};
    let dir = PathBuf::from(std::env::var("MOTION_AEP_SAMPLES").expect("set $MOTION_AEP_SAMPLES"));
    // Per belief: how many held, and where one did not.
    let mut held: BTreeMap<&str, usize> = BTreeMap::new();
    let mut broken: Vec<String> = Vec::new();
    let mut check = |belief: &'static str, holds: bool, at: &dyn Fn() -> String| match holds {
        true => *held.entry(belief).or_default() += 1,
        false => broken.push(format!("{belief}: {}", at())),
    };
    let mut seen = 0;
    for path in projects(&dir) {
        let Ok(project) = motion_import::decode(&fs::read(&path).expect("read a sample")) else {
            continue;
        };
        let mut docs = Vec::new();
        for item in &project.items {
            if let ItemKind::Composition(comp) = &item.kind {
                comp.layers
                    .iter()
                    .for_each(|layer| documents(&layer.properties, &mut docs));
            }
        }
        for doc in docs.into_iter().filter(|doc| !doc.lines.is_empty()) {
            seen += 1;
            let at = || format!("{} {:?}", path.display(), doc.text);
            // B1: a line's origin is on its baseline.
            for line in &doc.lines {
                check(
                    "B1 line origin y is its baseline",
                    (line.origin[1] - line.baseline).abs() < 1e-3,
                    &at,
                );
            }
            // B2: the lines count every character, each paragraph's end as one.
            let counted: usize = doc
                .lines
                .iter()
                .flat_map(|l| &l.segments)
                .map(|s| s.characters)
                .sum();
            let units = doc.text.encode_utf16().count();
            check(
                "B2 lines count the text and one closing character",
                counted == units + 1,
                &|| format!("{} ({counted} counted, {units} in the text)", at()),
            );
            // B3: point text's segments start where their alignment puts
            // them, aligned without the tracking after the last character,
            // which the stored width includes.
            let one_rule = doc
                .paragraphs
                .iter()
                .map(|p| p.style.justification)
                .reduce(|a, b| if a == b { a } else { Justification::JustifyAll });
            if let (TextFrame::Point, Some(rule)) = (doc.frame, one_rule) {
                let share = match rule {
                    Justification::Left => Some(0.0),
                    Justification::Center => Some(0.5),
                    Justification::Right => Some(1.0),
                    _ => None,
                };
                let trailing = doc
                    .characters
                    .last()
                    .map_or(0.0, |run| run.style.tracking / 1000.0 * run.style.size);
                for (line, share) in doc.lines.iter().zip(std::iter::repeat(share).flatten()) {
                    let (Some(first), width) = (
                        line.segments.first(),
                        line.segments.iter().map(|s| s.width).sum::<f64>(),
                    ) else {
                        continue;
                    };
                    check(
                        "B3 point text segments start at their alignment",
                        (first.origin[0] + share * (width - trailing)).abs() < 0.01,
                        &|| format!("{} (starts {}, width {width})", at(), first.origin[0]),
                    );
                }
            }
        }
    }
    println!("{seen} stored layouts");
    for (belief, count) in &held {
        println!("  {belief}: held {count} times");
    }
    assert!(
        broken.is_empty(),
        "beliefs that did not hold:\n{}",
        broken.join("\n")
    );
}
