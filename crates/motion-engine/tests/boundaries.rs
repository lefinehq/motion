//! D37: engines stay behind our own types. No crate's public API names an
//! engine (the shaper, the rasteriser, the parsers): a `pub` line in any
//! crate's source that names one fails here, with where. Vocabulary crates
//! (`kurbo`, `peniko`, `linebender_resource_handle`) are allowed.

use std::fs;
use std::path::{Path, PathBuf};

/// The engines no public API may name.
const ENGINES: &[&str] = &[
    "parley",
    "fontique",
    "harfrust",
    "skrifa",
    "read_fonts",
    "vello_cpu",
    "vello_common",
    "fearless_simd",
    "binrw",
    "winnow",
    "png",
];

/// The workspace's crates folder.
const CRATES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

/// Every Rust file under `dir`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read a source folder") {
        let path = entry.expect("a folder entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_public_api_names_an_engine() {
    let mut files = Vec::new();
    for krate in fs::read_dir(CRATES).expect("read the crates folder") {
        let src = krate.expect("a crate").path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    let mut found = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read a source file");
        for (number, line) in text.lines().enumerate() {
            let line = line.trim_start();
            // `pub(crate)` and narrower are not public.
            if !line.starts_with("pub ") {
                continue;
            }
            for engine in ENGINES {
                if line.contains(&format!("{engine}::")) || line.contains(&format!("use {engine}"))
                {
                    found.push(format!("{}:{}: {line}", file.display(), number + 1));
                }
            }
        }
    }
    assert!(files.len() > 50, "only {} source files", files.len());
    assert!(
        found.is_empty(),
        "public lines that name an engine:\n{}",
        found.join("\n")
    );
}
