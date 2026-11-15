//! Unit tests for `src/rules/judder.rs`.

use super::*;
use kurbo::{Affine, Point};
use rstest::rstest;

/// A 1000x1080 canvas, so the limit is 11 px.
fn graph() -> SceneGraph {
    SceneGraph {
        root: CompGraph {
            id: motion_format::address::SCENE.into(),
            width: 1000.0,
            height: 1080.0,
            fps: 30.0,
            duration_frames: 30.0,
            background: [0.0, 0.0, 0.0, 1.0],
            shutter: motion_format::shorthand::Shutter::DEFAULT,
            nodes: vec![],
        },
        comps: vec![],
    }
}

/// A 100-px square at `x` that moved itself there.
fn at(frame: u32, x: f64, blurred: bool) -> Sample {
    let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
        .map(|(cx, cy)| Point::new(x + cx, cy));
    Sample {
        frame,
        shown: true,
        opacity: 1.0,
        corners,
        local: Affine::translate((x, 0.0)),
        chars: None,
        blurred,
    }
}

#[rstest]
#[case::within_the_limit(10.0, false, None)]
#[case::on_the_limit(11.0, false, None)]
#[case::past_the_limit(24.0, false, Some(24.0))]
#[case::blurred_steps_are_smooth(24.0, true, None)]
#[case::standing_still(0.0, false, None)]
fn a_step_judders_past_the_limit(
    #[case] dx: f64,
    #[case] blurred: bool,
    #[case] found: Option<f64>,
) {
    let step = match step(&at(0, 0.0, blurred), &at(1, dx, blurred), &graph()) {
        Measured::Found(f) => Some(f.value),
        Measured::Passed | Measured::Skipped => None,
    };
    assert_eq!(step, found);
}

#[test]
fn moving_only_with_what_holds_it_is_not_its_own_judder() {
    let before = at(0, 0.0, false);
    let now = Sample {
        local: before.local,
        ..at(1, 40.0, false)
    };
    assert_eq!(step(&before, &now, &graph()), Measured::Skipped);
}

#[test]
fn the_limit_scales_with_the_canvas_height() {
    let mut tall = graph();
    tall.root.height = 2160.0;
    assert_eq!(judder_limit(&tall), 22.0);
    assert_eq!(
        step(&at(0, 0.0, false), &at(1, 20.0, false), &tall),
        Measured::Passed
    );
}

#[test]
fn it_prints_whole_px_per_frame_against_the_limit() {
    let printed = (JUDDER.measure)(Finding {
        value: 23.2,
        limit: 11.0,
    });
    assert_eq!(printed.to_string(), "24px/f >11");
}

#[test]
fn the_fix_turns_motion_blur_on_where_the_shutter_is_open() {
    let mut scene = motion_format::parse_document(
        r#"{"motion":1,"dur":"1s","layers":[{"id":"card","rect":"10x10"}]}"#,
    )
    .unwrap()
    .scene()
    .clone();
    let mut typesetter = motion_text::Typesetter::new(motion_text::FontSet::builtin());
    let graph = motion_scene::compile(&scene, &mut typesetter).unwrap();
    let address: Address = "card".parse().unwrap();
    let (node, comp) = graph.node_at(&["card"]).unwrap();
    assert_eq!(
        fix(&address, node, comp).as_deref(),
        Some("card mblur=true")
    );

    scene.root.shutter = "0".parse().unwrap();
    let closed = motion_scene::compile(&scene, &mut typesetter).unwrap();
    let (node, comp) = closed.node_at(&["card"]).unwrap();
    assert_eq!(fix(&address, node, comp), None);
}
