//! `--threads` and `NAKKASH_THREADS` cap every command's threads, not only
//! the drawing of a render's frames: `check`, `diff`, `slots --verify` and
//! a render's video encoder keep to one thread when told one ([D90]).
//!
//! What the process spent on processors, as `--stats` reads it, cannot pass
//! its wall time on one thread but by the moments a waiting thread wakes;
//! on more, it would take the machine's cores.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");

/// An empty folder of the test's own.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Runs `nakkash --stats` with `args` in the examples folder, with the
/// environment's caps taken away and `env` set, and returns its wall time
/// and its time on processors, user and system, in seconds.
fn cost(env: &[(&str, &str)], args: &[&str]) -> (f64, f64) {
    let out = Command::new(env!("CARGO_BIN_EXE_nakkash"))
        .env_remove("NAKKASH_LOG")
        .env_remove("NAKKASH_THREADS")
        .env_remove("RAYON_NUM_THREADS")
        .envs(env.iter().copied())
        .current_dir(EXAMPLES)
        .arg("--stats")
        .args(args)
        .output()
        .expect("run nakkash");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{args:?}:\n{stderr}");
    let line = stderr
        .lines()
        .find(|l| l.starts_with("stats: time:"))
        .unwrap_or_else(|| panic!("no time line in:\n{stderr}"));
    let after = |label: &str| -> f64 {
        let at = line.find(label).expect("a label of the time line") + label.len();
        line[at..]
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no number after `{label}` in {line}"))
    };
    (after("wall "), after("user ") + after("system "))
}

/// Asserts that a run took no more than one thread's time: its time on
/// processors within a quarter more than its wall time, and a fifth of a
/// second for the process starting and waking threads.
fn on_one_thread(what: &str, (wall, processors): (f64, f64)) {
    assert!(
        processors <= 1.25 * wall + 0.2,
        "{what}: {processors:.2} s on processors in {wall:.2} s, more than one thread's"
    );
}

#[test]
fn every_command_keeps_to_the_threads_it_is_given() {
    let dir = scratch("threads-commands");
    let film = dir.join("launch.mp4");
    let film = film.to_str().expect("a UTF-8 path");
    let reference = dir.join("reference");
    let reference = reference.to_str().expect("a UTF-8 path");
    let one = ["--threads", "1"];
    let with = |args: &[&'static str]| [&one[..], args].concat();
    // The reference frames `diff` compares with.
    cost(
        &[],
        &[
            "--threads",
            "2",
            "render",
            "launch.json",
            "--frames",
            "0-5",
            "--out",
            &format!("{reference}/"),
        ],
    );
    on_one_thread("check", cost(&[], &with(&["check", "launch.json"])));
    on_one_thread(
        "diff",
        cost(
            &[],
            &[
                &with(&["diff", "launch.json", "--fps", "30", "--reference"])[..],
                &[reference],
            ]
            .concat(),
        ),
    );
    on_one_thread(
        "slots --verify",
        cost(&[], &with(&["slots", "social-916.json", "--verify"])),
    );
    // The encoder's own threads too: AV1 on every core before.
    on_one_thread(
        "render to video",
        cost(
            &[],
            &[&with(&["render", "launch.json", "--out"])[..], &[film]].concat(),
        ),
    );
    // The environment's cap holds where no `--threads` is given.
    on_one_thread(
        "render to video under NAKKASH_THREADS=1",
        cost(
            &[("NAKKASH_THREADS", "1")],
            &["render", "launch.json", "--out", film],
        ),
    );
    let _ = fs::remove_dir_all(&dir);
}
