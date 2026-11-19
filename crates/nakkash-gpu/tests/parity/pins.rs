//! Corner pins ([D89]) drawn whole on the GPU and on the CPU: a pinned
//! layer's plane, effects before and after its pin, a pin on an adjusting
//! layer that lays what is below on the grid, and a pin blurred over its
//! moments. Each frame is held to one 8-bit level, as an example's is.
//!
//! [D89]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d89-a-corner-pin-lays-its-layers-plane-through-a-homography

use nakkash_raster::kurbo::{Affine, Circle, Rect, Shape};
use nakkash_raster::{
    Color, DrawStyle, Effect, FillRule, Frame, Layer, Node, Paint, ParamValue, PathDraw,
    RenderOptions, render_on,
};

use super::gpu;

/// A filled outline in `colour`.
fn fill(path: nakkash_raster::kurbo::BezPath, colour: Color) -> Node {
    Node::Path(PathDraw {
        path,
        paint: Paint::Solid(colour),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// A 160 by 120 card with edges every way.
fn card() -> Layer {
    let (w, h) = (160.0, 120.0);
    Layer {
        bounds: Some(Rect::new(0.0, 0.0, w, h)),
        ..Layer::new(vec![
            fill(
                Rect::new(0.0, 0.0, w, h).to_path(0.1),
                Color::rgba(0.95, 0.94, 0.9, 1.0),
            ),
            fill(
                Circle::new((55.0, 60.0), 34.0).to_path(0.1),
                Color::rgba(0.1, 0.1, 0.2, 1.0),
            ),
            fill(
                Rect::new(100.0, 20.0, 112.0, 100.0).to_path(0.1),
                Color::rgba(0.9, 0.2, 0.1, 1.0),
            ),
        ])
    }
}

/// A pin taking the card's box to `to`.
fn pin(to: [[f64; 2]; 4]) -> Effect {
    let rest = [[0.0, 0.0], [160.0, 0.0], [160.0, 120.0], [0.0, 120.0]];
    let names = ["top-left", "top-right", "bottom-right", "bottom-left"];
    let params: Vec<(&str, ParamValue)> = (0..4)
        .map(|k| {
            let d = [to[k][0] - rest[k][0], to[k][1] - rest[k][1]];
            (names[k], ParamValue::Point([d[0] as f32, d[1] as f32]))
        })
        .collect();
    Effect::new("corner-pin", &params).expect("a pin")
}

/// A small quad, shrinking the card, and a large one enlarging it.
const SMALL: [[f64; 2]; 4] = [[40.0, 30.0], [110.0, 42.0], [104.0, 120.0], [36.0, 104.0]];
const LARGE: [[f64; 2]; 4] = [[10.0, 8.0], [300.0, 30.0], [290.0, 230.0], [20.0, 200.0]];
/// The card's box moved by a fraction of a px, at its own size.
const KEPT: [[f64; 2]; 4] = [[80.3, 60.7], [240.3, 60.7], [240.3, 180.7], [80.3, 180.7]];

/// The largest difference of two frames' channels, in 8-bit levels.
fn largest(frame: &Frame, width: u32, height: u32) -> u8 {
    let gpu = gpu().expect("checked by the caller");
    let options = RenderOptions::default();
    let cpu = render_on(frame, width, height, options, None).expect("drawn");
    let on_gpu = render_on(frame, width, height, options, Some(gpu)).expect("drawn on the GPU");
    cpu.pixels()
        .iter()
        .zip(on_gpu.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0)
}

#[test]
fn pinned_frames_draw_within_a_level_of_the_cpu() {
    if gpu().is_none() {
        return;
    }
    let blur = || Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(4.0))]).unwrap();
    let shadow = || Effect::new("drop-shadow", &[("distance", ParamValue::Number(6.0))]).unwrap();
    let moments = |to: [[f64; 2]; 4]| [-6.0, 0.0, 6.0].map(|dx| pin(to.map(|[x, y]| [x + dx, y])));
    // `KEPT` with each corner moved a little differently, so a moment's
    // map relative to the frame's is near a whole px but not one.
    let nudged = |dx: f64| {
        let mut k = 0.0;
        pin(KEPT.map(|[x, y]| {
            k += 1.0;
            [x + dx * (1.0 + 0.07 * k), y + dx * (0.5 - 0.05 * k)]
        }))
    };
    let cases: Vec<(&str, Vec<Node>)> = vec![
        (
            "shrunk",
            vec![Node::Layer(Layer {
                effects: vec![pin(SMALL)],
                ..card()
            })],
        ),
        (
            "enlarged",
            vec![Node::Layer(Layer {
                effects: vec![pin(LARGE)],
                ..card()
            })],
        ),
        (
            "moved and turned",
            vec![Node::Layer(Layer {
                transform: Affine::translate((60.0, 20.0)) * Affine::rotate(0.2),
                effects: vec![pin(SMALL)],
                ..card()
            })],
        ),
        (
            "effects before and after",
            vec![Node::Layer(Layer {
                effects: vec![blur(), pin(SMALL), shadow()],
                ..card()
            })],
        ),
        (
            "on an adjusting layer",
            vec![
                Node::Layer(Layer {
                    isolate: true,
                    ..card()
                }),
                Node::Layer(Layer {
                    adjust: true,
                    effects: vec![
                        Effect::new(
                            "corner-pin",
                            &[("top-right", ParamValue::Point([-40.0, 20.0]))],
                        )
                        .unwrap()
                        .with_rest(Rect::new(0.0, 0.0, 160.0, 120.0)),
                    ],
                    ..Layer::new(Vec::new())
                }),
            ],
        ),
        (
            "blurred over its moments",
            vec![Node::Layer(Layer {
                effects: vec![pin(SMALL).with_moments(moments(SMALL)).unwrap()],
                motion: vec![
                    Affine::translate((0.0, -3.0)),
                    Affine::IDENTITY,
                    Affine::translate((0.0, 3.0)),
                ],
                ..card()
            })],
        ),
        (
            "blurred with an effect after",
            vec![Node::Layer(Layer {
                effects: vec![pin(SMALL).with_moments(moments(SMALL)).unwrap(), shadow()],
                motion: vec![Affine::IDENTITY; 3],
                ..card()
            })],
        ),
        // Footprints of whole px, where a probe more or a copy coarser
        // would turn on the last bit (`BOX_SLACK`): a pin that keeps the
        // card's size, over a shutter that moves it a fraction of a px.
        (
            "kept at its size, blurred with an effect after",
            vec![Node::Layer(Layer {
                effects: vec![
                    pin(KEPT)
                        .with_moments([-0.4, 0.0, 0.4].map(nudged))
                        .unwrap(),
                    shadow(),
                ],
                motion: vec![Affine::IDENTITY; 3],
                ..card()
            })],
        ),
    ];
    for (name, nodes) in cases {
        let frame = Frame {
            background: Some(Color::rgba(1.0, 1.0, 1.0, 1.0)),
            nodes,
            ..Frame::default()
        };
        let worst = largest(&frame, 320, 240);
        eprintln!("pinned frame, {name}: largest {worst} levels");
        assert!(worst <= 1, "{name}: frames differ by {worst} levels");
    }
}
