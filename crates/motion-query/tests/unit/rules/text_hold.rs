//! Unit tests for `src/rules/text_hold.rs`.

use super::*;
use kurbo::{Affine, Point};
use rstest::rstest;

/// A 30 fps scene on a 1080-px-tall canvas.
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

fn text(chars: Option<usize>, opacity: f64, x: f64) -> Sample {
    Sample {
        frame: 0,
        shown: true,
        opacity,
        corners: [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
            .map(|(cx, cy)| Point::new(x + cx, cy)),
        local: Affine::translate((x, 0.0)),
        chars,
        blurred: false,
    }
}

#[rstest]
#[case::readable(text(Some(5), 1.0, 0.0), true)]
#[case::not_text(text(None, 1.0, 0.0), false)]
#[case::fading(text(Some(5), 0.5, 0.0), false)]
#[case::moving_fast(text(Some(5), 1.0, 40.0), false)]
#[case::moving_slowly(text(Some(5), 1.0, 5.0), true)]
fn text_is_readable_when_shown_opaque_and_steady(#[case] now: Sample, #[case] readable: bool) {
    let before = text(Some(5), 1.0, 0.0);
    assert_eq!(holds(&now, Some(&before), &graph()), readable);
}

/// Frames at 30 fps a text of `chars` characters holds, and what is found.
#[rstest]
// 10 characters need 5/6 s: 25 frames.
#[case::short_text_takes_the_shortest_hold(10, 25, None)]
#[case::short_text_shown_too_briefly(10, 12, Some((0.4, 5.0 / 6.0)))]
// 60 characters need 3 s: 90 frames.
#[case::long_text_takes_its_reading_time(60, 90, None)]
#[case::long_text_shown_too_briefly(60, 45, Some((1.5, 3.0)))]
fn a_stretch_is_judged_by_its_reading_time(
    #[case] chars: usize,
    #[case] frames: u32,
    #[case] found: Option<(f64, f64)>,
) {
    let judged = judge(frames, &text(Some(chars), 1.0, 0.0), &graph());
    assert_eq!(judged.map(|f| (f.value, f.limit)), found);
}

#[test]
fn it_prints_tenths_of_a_second_rounded_away_from_the_limit() {
    let printed = (TEXT_HOLD.measure)(Finding {
        value: 0.43,
        limit: 5.0 / 6.0,
    });
    assert_eq!(printed.to_string(), "0.4s <0.9");
}
