//! Unit tests for `src/rules/crowd.rs`.

use super::*;
use kurbo::{Affine, Point};
use rstest::rstest;

#[rstest]
#[case::four_is_fine(4, None)]
#[case::five_is_a_crowd(5, Some(5.0))]
fn more_than_four_movers_is_a_crowd(#[case] movers: usize, #[case] found: Option<f64>) {
    assert_eq!(judge(movers).map(|f| f.value), found);
}

#[test]
fn it_prints_the_count_against_the_limit() {
    let printed = measure(judge(6).unwrap());
    assert_eq!(printed.to_string(), "6 >4");
}

fn at(x: f64, own: bool) -> Sample {
    Sample {
        frame: 0,
        shown: true,
        opacity: 1.0,
        corners: [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
            .map(|(cx, cy)| Point::new(x + cx, cy)),
        local: if own {
            Affine::translate((x, 0.0))
        } else {
            Affine::IDENTITY
        },
        chars: None,
        blurred: false,
    }
}

#[test]
fn only_layers_that_move_on_their_own_and_noticeably_catch_the_eye() {
    let graph = SceneGraph {
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
    };
    assert!(catches_the_eye(&at(0.0, true), &at(5.0, true), &graph));
    assert!(!catches_the_eye(&at(0.0, true), &at(1.0, true), &graph));
    assert!(!catches_the_eye(&at(0.0, false), &at(5.0, false), &graph));
}
