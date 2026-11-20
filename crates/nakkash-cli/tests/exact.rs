//! The precise path gives the same bits however a render runs ([D90],
//! rule 7): on one thread or many, and with every pooled grid clobbered.
//! The render machine runs the same comparison over the corpus.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::process::Command;

const SCENES: &[(&str, &str)] = &[("launch.json", "0-5"), ("advanced/camera-depth.json", "12")];

/// The digests `render --digest` prints for `frames` of `scene` on
/// `threads` threads, clobbering pooled grids or not.
fn digests(scene: &str, frames: &str, threads: usize, clobber: bool) -> String {
    let scene = format!("{}/../../examples/{scene}", env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_nakkash"));
    command
        .env_remove("NAKKASH_LOG")
        .env_remove("NAKKASH_CLOBBER");
    if clobber {
        command.env("NAKKASH_CLOBBER", "1");
    }
    let threads = threads.to_string();
    let run = command
        .args(["--threads", &threads, "render", &scene, "--frames", frames])
        .arg("--digest")
        .output()
        .expect("run nakkash");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).expect("digests are text")
}

#[test]
fn a_render_gives_the_same_frames_on_every_thread_count_with_grids_clobbered() {
    for &(scene, frames) in SCENES {
        let reference = digests(scene, frames, 1, false);
        assert!(!reference.is_empty());
        for (threads, clobber) in [(2, false), (3, true), (8, true)] {
            assert_eq!(
                digests(scene, frames, threads, clobber),
                reference,
                "{scene} on {threads} threads, clobbered: {clobber}"
            );
        }
    }
}
