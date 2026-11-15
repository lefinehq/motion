//! D37, D38, D39, D46 and D48. Engines stay behind our own types. No crate's public API names an
//! engine (the shaper, the rasteriser, the parsers): a `pub` line in any
//! crate's source that names one fails here, with where. Vocabulary crates
//! (`kurbo`, `peniko`, `linebender_resource_handle`) are allowed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    "boa_engine",
    "boa_parser",
    "boa_ast",
    "boa_interner",
    "rav1e",
    "mp4_atom",
    "objc2_core_foundation",
    "objc2_core_media",
    "objc2_core_video",
    "objc2_video_toolbox",
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

/// Network clients (D39).
const NETWORK: &[&str] = &["ureq", "reqwest", "hyper", "curl", "isahc", "attohttpc"];

/// The script engine (D38).
const SCRIPT: &[&str] = &["boa_engine", "boa_parser", "boa_ast", "boa_interner"];

/// Every crate of the workspace, by name.
fn crates() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(CRATES)
        .expect("read the crates folder")
        .map(|entry| entry.expect("a crate").path())
        .filter(|dir| dir.join("Cargo.toml").is_file())
        .map(|dir| dir.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The packages `krate` links in a build of its own with its default
/// features, dev-dependencies left out: its resolved graph, as Cargo builds
/// it, so a dependency reached through another crate or a feature counts.
fn linked(krate: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "--offline", "--quiet", "-p", krate])
        .args(["-e", "normal", "--prefix", "none", "--format", "{p}"])
        .current_dir(CRATES)
        .output()
        .expect("run cargo tree");
    assert!(
        out.status.success(),
        "cargo tree -p {krate}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The crates of the workspace, other than `allowed`, that link one of
/// `banned`, each with what it links.
fn linking(banned: &[&str], allowed: &[&str]) -> Vec<String> {
    crates()
        .into_iter()
        .filter(|krate| !allowed.contains(&krate.as_str()))
        .filter_map(|krate| {
            let found: Vec<String> = linked(&krate)
                .into_iter()
                .filter(|name| banned.contains(&name.as_str()))
                .collect();
            (!found.is_empty()).then(|| format!("{krate}: {}", found.join(", ")))
        })
        .collect()
}

/// D39: a render never reaches the network. Only the CLI, which runs
/// `motion fonts fetch` and turns on the font crate's `fetch` feature, links
/// a network client.
#[test]
fn nothing_that_renders_can_reach_the_network() {
    let found = linking(NETWORK, &["motion-cli"]);
    assert!(
        found.is_empty(),
        "only the CLI may reach the network (D39):\n{}",
        found.join("\n")
    );
}

/// D38: a render never runs JavaScript. Only the script crate itself and
/// the CLI, which turns on the import's `bake` feature, link the script
/// engine; the engine crate renders and imports without it.
#[test]
fn nothing_that_renders_links_the_script_engine() {
    let found = linking(SCRIPT, &["motion-cli", "motion-expr"]);
    assert!(
        found.is_empty(),
        "only the CLI's import may run expressions (D38):\n{}",
        found.join("\n")
    );
}

/// D46: video is the CLI's last step. No crate that reads, compiles or
/// draws a scene links the encoder; frames reach it through the CLI.
#[test]
fn nothing_that_renders_links_the_encoder() {
    let found = linking(&["motion-encode"], &["motion-cli", "motion-encode"]);
    assert!(
        found.is_empty(),
        "only the CLI may link the encoder (D46):\n{}",
        found.join("\n")
    );
}

/// D46 and D60: the binding to the system's codecs is reached only through
/// the crates that put it behind their codec traits, `motion-encode` for
/// encoding and `motion-video` for decoding, so nothing that draws a scene
/// calls its encoder.
#[test]
fn the_system_codecs_are_reached_only_through_the_codec_crates() {
    let through = ["motion-encode", "motion-video", "motion-videotoolbox"];
    let found: Vec<String> = crates()
        .into_iter()
        .filter(|krate| !through.contains(&krate.as_str()))
        .filter(|krate| direct(krate).iter().any(|d| d == "motion-videotoolbox"))
        .collect();
    assert!(
        found.is_empty(),
        "crates that depend on motion-videotoolbox directly (D46, D60): {}",
        found.join(", ")
    );
}

/// The packages `krate` depends on directly, dev-dependencies left out.
fn direct(krate: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "--offline", "--quiet", "-p", krate, "--depth", "1"])
        .args(["-e", "normal", "--prefix", "none", "--format", "{p}"])
        .current_dir(CRATES)
        .output()
        .expect("run cargo tree");
    assert!(
        out.status.success(),
        "cargo tree -p {krate}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect()
}

/// Encoders under the GPL, or software encoders of patented codecs
/// compiled in (D13).
const BARRED_CODECS: &[&str] = &[
    "x264",
    "x264-sys",
    "x265",
    "x265-sys",
    "ffmpeg-next",
    "ffmpeg-sys-next",
    "rsmpeg",
    "rusty_ffmpeg",
];

/// D13: the binary holds no GPL encoder and no H.264 or HEVC software
/// encoder compiled in. OpenH264 may come only as a binary loaded at run
/// time, so its `source` feature, which compiles it in, is never on.
#[test]
fn no_barred_encoder_is_compiled_in() {
    let found = linking(BARRED_CODECS, &[]);
    assert!(
        found.is_empty(),
        "barred encoders (D13):\n{}",
        found.join("\n")
    );
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "--quiet",
            "--workspace",
            "-e",
            "normal",
        ])
        .args(["--prefix", "none", "--format", "{p} [{f}]"])
        .current_dir(CRATES)
        .output()
        .expect("run cargo tree");
    assert!(
        out.status.success(),
        "cargo tree: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let compiled: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.starts_with("openh264"))
        .filter(|line| line.contains("[source") || line.contains(",source"))
        .map(str::to_owned)
        .collect();
    assert!(
        compiled.is_empty(),
        "OpenH264 compiled in (D13):\n{}",
        compiled.join("\n")
    );
}

/// The crates that may hold unsafe code: platform bindings (D48).
const UNSAFE_ALLOWED: &[&str] = &["motion-videotoolbox"];

/// D48: every other crate takes the workspace's lints, which forbid unsafe
/// code, so the compiler holds the line.
#[test]
fn only_platform_bindings_leave_the_workspace_lints() {
    let found: Vec<String> = crates()
        .into_iter()
        .filter(|krate| !UNSAFE_ALLOWED.contains(&krate.as_str()))
        .filter(|krate| {
            let manifest = fs::read_to_string(Path::new(CRATES).join(krate).join("Cargo.toml"))
                .expect("read a manifest");
            !manifest.contains("[lints]\nworkspace = true")
        })
        .collect();
    assert!(
        found.is_empty(),
        "crates without the workspace's lints (D48): {}",
        found.join(", ")
    );
    let workspace = fs::read_to_string(Path::new(CRATES).join("../Cargo.toml")).expect("read");
    assert!(
        workspace.contains("unsafe_code = \"forbid\""),
        "the workspace forbids unsafe code"
    );
}
