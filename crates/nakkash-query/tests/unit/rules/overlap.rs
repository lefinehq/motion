//! Unit tests for `src/rules/overlap.rs`.

use super::*;
use rstest::rstest;

fn graph() -> SceneGraph {
    SceneGraph {
        root: nakkash_scene::CompGraph {
            id: nakkash_format::address::SCENE.into(),
            width: 1920.0,
            height: 1080.0,
            fps: 30.0,
            duration_frames: 30.0,
            background: [0.0, 0.0, 0.0, 1.0],
            shutter: nakkash_format::shorthand::Shutter::DEFAULT,
            nodes: vec![],
        },
        comps: vec![],
    }
}

/// Two glyphs on a line from x 0 to 100, from `top` down to `top + 50`.
fn line(top: f64) -> Vec<Rect> {
    vec![
        Rect::new(0.0, top, 40.0, top + 50.0),
        Rect::new(60.0, top, 100.0, top + 50.0),
    ]
}

#[rstest]
#[case::apart(line(100.0), None)]
#[case::touching(line(50.0), None)]
#[case::within_a_pixel(line(49.0), None)]
#[case::running_in(line(44.0), Some(6.0))]
// Their reaches overlap, but the glyphs sit side by side.
#[case::between_the_glyphs(vec![Rect::new(42.0, 20.0, 58.0, 70.0)], None)]
fn ink_runs_in_only_past_touching(#[case] other: Vec<Rect>, #[case] depth: Option<f64>) {
    let found = judge(&line(0.0), &other, &graph()).map(|f| f.value);
    assert_eq!(found, depth);
}

#[test]
fn nothing_drawn_never_overlaps() {
    assert_eq!(judge(&line(0.0), &[], &graph()), None);
}
