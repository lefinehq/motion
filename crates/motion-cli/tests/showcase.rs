//! The showcase scenes in `examples/showcase/`: long, dense pieces written
//! from scratch, each passing `motion check` with nothing found.

use std::process::Command;

use motion_engine::Session;

/// The data story's scene file.
const DATA_STORY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/showcase/data-story.json"
);

/// The data story passes `check` with nothing found.
///
/// `motion check` runs the scene's rules over every frame, which takes
/// milliseconds, and then its pixel gates (`contrast`, `flash`), which draw
/// all 1146 frames: about 30 s on ten cores when optimised, and far beyond a
/// test's budget in a debug build. So a debug build runs the rules alone, on
/// every frame, and an optimised one (`cargo nextest run --release`) runs the
/// whole command and holds it to a single `OK` line and exit code 0.
#[test]
fn data_story_passes_check() {
    if cfg!(debug_assertions) {
        let report = Session::open(DATA_STORY)
            .expect("open the data story")
            .check()
            .expect("check the data story");
        assert!(report.violations.is_empty(), "{report}");
        assert!(report.passed > 0, "{report}");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_motion"))
        .env_remove("MOTION_LOG")
        .args(["check", DATA_STORY])
        .output()
        .expect("run motion");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("OK ") && text.lines().count() == 1,
        "{text}"
    );
    assert_eq!(out.status.code(), Some(0));
}
