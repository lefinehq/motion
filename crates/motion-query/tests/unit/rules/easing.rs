//! Unit tests for `src/rules/easing.rs`.

use super::*;
use rstest::rstest;

/// A 1080-px-tall canvas, where a px is a px.
fn graph() -> SceneGraph {
    SceneGraph {
        root: motion_scene::CompGraph {
            id: motion_format::address::SCENE.into(),
            width: 1920.0,
            height: 1080.0,
            fps: 30.0,
            duration_frames: 300.0,
            background: [0.0, 0.0, 0.0, 1.0],
            shutter: motion_format::shorthand::Shutter::DEFAULT,
            nodes: vec![],
        },
        comps: vec![],
    }
}

/// A move over `speeds`, one per step, from frame 0.
fn moving(speeds: &[f64]) -> Move {
    Move {
        first: 0,
        last: speeds.len() as u32,
        total: speeds.iter().sum(),
        peak: speeds.iter().copied().fold(0.0, f64::max),
    }
}

#[rstest]
#[case::constant_speed(&[10.0; 8], Measured::Found(Finding { value: 1.0, limit: 1.4 }))]
#[case::eased_out(&[20.0, 16.0, 12.0, 9.0, 6.0, 4.0, 2.0, 1.0], Measured::Passed)]
#[case::too_brief_to_judge(&[10.0; 5], Measured::Skipped)]
#[case::a_drift(&[1.0; 30], Measured::Skipped)]
fn a_move_eases_or_is_found(#[case] speeds: &[f64], #[case] measured: Measured) {
    assert_eq!(judge(&moving(speeds), &graph()), measured);
}

#[test]
fn it_prints_two_decimals_against_the_limit() {
    let printed = (EASING.measure)(Finding {
        value: 1.0,
        limit: 1.4,
    });
    assert_eq!(printed.to_string(), "1 <1.4");
}
