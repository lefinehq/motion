//! D37 and D39. Engines stay behind our own types. No crate's public API names an
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
    "ureq",
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

/// Network clients: only the font fetcher may use one (D39).
const NETWORK: &[&str] = &["ureq", "reqwest", "hyper", "curl", "isahc", "attohttpc"];

/// The crates that may reach the network: the CLI, which runs `motion fonts
/// fetch`, and the font crate, whose `fetch` feature is optional.
const MAY_FETCH: &[&str] = &["motion-cli", "motion-fonts"];

/// The dependency lines of the manifest `text`: its `[dependencies]` and
/// target-specific ones, not its dev-dependencies.
fn dependency_lines(text: &str) -> Vec<&str> {
    let mut section = "";
    let mut lines = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
            continue;
        }
        let normal = section == "[dependencies]"
            || (section.starts_with("[target.") && section.ends_with(".dependencies]"));
        if normal && !line.is_empty() && !line.starts_with('#') {
            lines.push(line);
        }
    }
    lines
}

/// D39: a render never reaches the network. No crate but those in
/// `MAY_FETCH` depends on a network client or on the font crate's `fetch`
/// feature, and the font crate's own client is optional, behind `fetch`.
#[test]
fn nothing_that_renders_can_reach_the_network() {
    let mut found = Vec::new();
    for krate in fs::read_dir(CRATES).expect("read the crates folder") {
        let dir = krate.expect("a crate").path();
        let manifest = dir.join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        for line in dependency_lines(&text) {
            let dependency = line.split(['=', ' ', '.']).next().unwrap_or("");
            let network = NETWORK.contains(&dependency);
            let fetching = dependency == "motion-fonts" && line.contains("fetch");
            // The font crate may only have its client behind a feature.
            let optional = name != "motion-fonts" || !network || line.contains("optional = true");
            let allowed = MAY_FETCH.contains(&name.as_str()) && optional;
            if (network || fetching) && !allowed {
                found.push(format!("{}: {line}", manifest.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "only the CLI and the font crate's `fetch` feature may reach the network (D39):\n{}",
        found.join("\n")
    );
}
