//! The resource gates of D90, on the real binary: a render under a memory
//! budget stays within it in resident memory and draws the same frames as
//! one without, and a render to `--digest` writes nothing to storage.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// Effects on 3D planes on a 1920x1080 canvas: a few hundred MiB a frame.
const HEAVY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/advanced/camera-depth.json"
);
const LAUNCH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/launch.json");

/// An empty folder of the test's own.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Runs `nakkash` in `dir`, with `dir` as its temporary folder too.
fn nakkash_in(dir: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nakkash"))
        .env_remove("NAKKASH_LOG")
        .env("TMPDIR", dir)
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run nakkash")
}

/// The number `--stats` printed after `label` on the line `stats: kind:`.
fn figure(stderr: &str, kind: &str, label: &str) -> f64 {
    let line = stderr
        .lines()
        .find(|line| line.starts_with(&format!("stats: {kind}:")))
        .unwrap_or_else(|| panic!("no `{kind}` line in:\n{stderr}"));
    let rest = &line[line
        .find(label)
        .unwrap_or_else(|| panic!("no `{label}` in {line}"))
        + label.len()..];
    rest.split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no number after `{label}` in {line}"))
}

/// Renders the heavy scene's frames 10 to 13 on four threads with a budget
/// of `mib`, and returns their digests and the peak resident memory.
fn heavy(dir: &PathBuf, mib: u64) -> (String, f64) {
    let mib = mib.to_string();
    let run = nakkash_in(
        dir,
        &[
            "--threads",
            "4",
            "--mem-mib",
            &mib,
            "--stats",
            "render",
            HEAVY,
            "--frames",
            "10-13",
            "--digest",
        ],
    );
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    assert_eq!(run.status.code(), Some(0), "{stderr}");
    let peak = figure(&stderr, "memory", "peak resident");
    (String::from_utf8_lossy(&run.stdout).into_owned(), peak)
}

#[test]
fn a_render_under_a_budget_stays_within_it_and_draws_the_same_frames() {
    let dir = scratch("resources-budget");
    const BUDGET: u64 = 320;
    let (free, free_peak) = heavy(&dir, 1 << 20);
    let (held, held_peak) = heavy(&dir, BUDGET);
    assert_eq!(held, free, "a budget changed the frames");
    // Four frames side by side take more than the budget, so it bound.
    assert!(free_peak > BUDGET as f64, "unbounded peak {free_peak} MiB");
    assert!(
        held_peak <= BUDGET as f64,
        "peak resident {held_peak} MiB under a budget of {BUDGET} MiB"
    );
}

#[test]
fn a_render_to_digest_writes_nothing_to_storage() {
    let dir = scratch("resources-no-write");
    let run = nakkash_in(
        &dir,
        &["--stats", "render", LAUNCH, "--frames", "0-2", "--digest"],
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(0), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&run.stdout).lines().count(), 3);
    assert!(
        stderr.contains("stats: storage: written 0 B"),
        "bytes were written:\n{stderr}"
    );
    // No file in the working folder or the temporary one.
    assert_eq!(fs::read_dir(&dir).expect("read the folder").count(), 0);
}

#[test]
fn held_frames_are_copied_and_verify_mode_finds_them_equal() {
    let dir = scratch("resources-held");
    // A card that fades in over three frames, then holds still.
    fs::write(
        dir.join("held.json"),
        r##"{"nakkash":1,"size":"320x180","fps":10,"dur":"1s","bg":"#10131A","layers":[
        {"id":"card","rect":"120x60","fill":"#E9C46A","at":"center center","in":"fade 3f",
         "fx":[{"glow":20,"threshold":0.4,"intensity":0.8}]}]}"##,
    )
    .expect("write the scene");
    let run = |verify: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_nakkash"))
            .env_remove("NAKKASH_LOG")
            .env("NAKKASH_VERIFY_HITS", verify)
            .current_dir(&dir)
            .args(["--stats", "render", "held.json", "--digest"])
            .output()
            .expect("run nakkash");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(0), "{stderr}");
        (String::from_utf8_lossy(&out.stdout).into_owned(), stderr)
    };
    let (digests, stderr) = run("0");
    let copied = figure(&stderr, "plan", "frames copied");
    let lines: Vec<&str> = digests.lines().collect();
    assert_eq!(lines.len(), 10);
    // The still frames are one image, and all but one of them were copied.
    let still = lines[3..]
        .iter()
        .map(|l| l.rsplit(' ').next())
        .collect::<Vec<_>>();
    assert!(still.windows(2).all(|w| w[0] == w[1]), "{digests}");
    assert!(copied >= 6.0, "{stderr}");
    // Every copy drawn again and compared: the same frames, no error.
    let (verified, _) = run("1");
    assert_eq!(verified, digests);
}
