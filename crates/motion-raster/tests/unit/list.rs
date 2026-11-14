//! Unit tests for `src/list.rs`.

use super::*;
use crate::effect::{Effect, ParamValue};

#[test]
fn a_plain_layer_only_moves_its_children() {
    let layer = Layer {
        transform: Affine::translate((5.0, 5.0)),
        ..Layer::new(Vec::new())
    };
    assert!(!layer.is_isolated());
    assert_eq!(layer.opacity, 1.0);
    assert_eq!(layer.blend, BlendMode::Normal);
}

#[test]
fn opacity_blending_clips_effects_and_the_flag_each_isolate() {
    let plain = Layer::new(Vec::new());
    let fill = Effect::new("fill", &[("color", ParamValue::Color(Color::WHITE))]).unwrap();
    for layer in [
        Layer {
            opacity: 0.5,
            ..plain.clone()
        },
        Layer {
            blend: BlendMode::Screen,
            ..plain.clone()
        },
        Layer {
            clip: Some(Clip::Rect(Rect::new(0.0, 0.0, 1.0, 1.0))),
            ..plain.clone()
        },
        Layer {
            effects: vec![fill],
            ..plain.clone()
        },
        Layer {
            isolate: true,
            ..plain.clone()
        },
    ] {
        assert!(layer.is_isolated(), "{layer:?}");
    }
}

#[test]
fn a_clip_gives_its_outline_and_rule() {
    let (rect, rule) = Clip::Rect(Rect::new(0.0, 0.0, 10.0, 20.0)).outline();
    assert_eq!(rule, FillRule::NonZero);
    assert_eq!(rect.bounding_box(), Rect::new(0.0, 0.0, 10.0, 20.0));
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((4.0, 0.0));
    path.line_to((0.0, 4.0));
    path.close_path();
    let (outline, rule) = Clip::Path(path.clone(), FillRule::EvenOdd).outline();
    assert_eq!((outline, rule), (path, FillRule::EvenOdd));
}

#[test]
fn a_default_frame_is_empty_transparent_and_gamma() {
    let frame = Frame::default();
    assert_eq!(frame.background, None);
    assert_eq!(frame.blending, Blending::Gamma);
    assert!(frame.nodes.is_empty());
}
