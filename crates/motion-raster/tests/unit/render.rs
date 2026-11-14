//! Unit tests for `src/render.rs`: sizes, settings, and the colours that
//! compositing gives, checked as numbers.

use super::*;
use crate::blend::BlendMode;
use crate::effect::{Effect, ParamValue};
use crate::list::Clip;
use crate::style::{FillRule, Paint};
use rstest::rstest;
use vello_cpu::kurbo::{Rect, Shape};

/// A square covering the whole of a `side`-pixel frame in `color`.
fn cover(side: f64, color: Color) -> Node {
    Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, side, side).to_path(0.1),
        paint: Paint::Solid(color),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// The centre pixel of `frame` drawn at 8x8 with golden options.
fn centre(frame: &Frame) -> [u8; 4] {
    render_with(frame, 8, 8, RenderOptions::GOLDEN)
        .unwrap()
        .pixel(4, 4)
        .unwrap()
}

/// Whether two pixels differ by at most one step per channel.
fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1)
}

#[rstest]
#[case::zero_width(0, 8)]
#[case::zero_height(8, 0)]
#[case::beyond_sixteen_bits(70_000, 8)]
fn sizes_must_fit_sixteen_bits_and_not_be_zero(#[case] w: u32, #[case] h: u32) {
    let e = render(&Frame::default(), w, h).unwrap_err();
    assert_eq!(e.kind, Kind::OutOfRange, "{w}x{h}");
    assert_eq!(e.expected.as_deref(), Some("1 to 65535 pixels"));
}

#[test]
fn linear_blending_is_reported_not_faked() {
    let frame = Frame {
        blending: Blending::Linear,
        ..Frame::default()
    };
    let e = render(&frame, 8, 8).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert!(e.message.contains("not supported yet"), "{e}");
}

#[test]
fn the_background_fills_the_frame_and_defaults_to_transparent() {
    let red = Frame {
        background: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        ..Frame::default()
    };
    assert_eq!(centre(&red), [255, 0, 0, 255]);
    assert_eq!(centre(&Frame::default()), [0, 0, 0, 0]);
}

#[test]
fn layer_opacity_and_a_transparent_layer() {
    let half = Layer {
        opacity: 0.5,
        ..Layer::new(vec![cover(8.0, Color::BLACK)])
    };
    let frame = Frame {
        background: Some(Color::WHITE),
        nodes: vec![Node::Layer(half)],
        ..Frame::default()
    };
    assert!(
        near(centre(&frame), [128, 128, 128, 255]),
        "{:?}",
        centre(&frame)
    );
    let gone = Layer {
        opacity: 0.0,
        ..Layer::new(vec![cover(8.0, Color::BLACK)])
    };
    let frame = Frame {
        background: Some(Color::WHITE),
        nodes: vec![Node::Layer(gone)],
        ..Frame::default()
    };
    assert_eq!(centre(&frame), [255, 255, 255, 255]);
}

#[test]
fn a_blend_mode_combines_with_what_is_below() {
    let top = Layer {
        blend: BlendMode::Multiply,
        ..Layer::new(vec![cover(8.0, Color::rgba(0.5, 1.0, 1.0, 1.0))])
    };
    let frame = Frame {
        background: Some(Color::rgba(1.0, 0.5, 1.0, 1.0)),
        nodes: vec![Node::Layer(top)],
        ..Frame::default()
    };
    assert!(
        near(centre(&frame), [128, 128, 255, 255]),
        "{:?}",
        centre(&frame)
    );
}

#[test]
fn the_fill_effect_mixes_towards_its_colour_and_keeps_alpha() {
    let fill = |opacity| {
        Effect::new(
            "fill",
            &[
                ("color", ParamValue::Color(Color::WHITE)),
                ("opacity", ParamValue::Number(opacity)),
            ],
        )
        .unwrap()
    };
    let layer = |opacity, content: Color| {
        Node::Layer(Layer {
            effects: vec![fill(opacity)],
            ..Layer::new(vec![cover(8.0, content)])
        })
    };
    let full = Frame {
        nodes: vec![layer(1.0, Color::rgba(1.0, 0.0, 0.0, 0.5))],
        ..Frame::default()
    };
    assert!(
        near(centre(&full), [255, 255, 255, 128]),
        "{:?}",
        centre(&full)
    );
    let half = Frame {
        nodes: vec![layer(0.5, Color::rgba(1.0, 0.0, 0.0, 1.0))],
        ..Frame::default()
    };
    assert!(
        near(centre(&half), [255, 128, 128, 255]),
        "{:?}",
        centre(&half)
    );
}

#[test]
fn a_clip_hides_what_lies_outside_it() {
    let clipped = Layer {
        clip: Some(Clip::Rect(Rect::new(0.0, 0.0, 4.0, 8.0))),
        ..Layer::new(vec![cover(8.0, Color::BLACK)])
    };
    let image = render_with(
        &Frame {
            nodes: vec![Node::Layer(clipped)],
            ..Frame::default()
        },
        8,
        8,
        RenderOptions::GOLDEN,
    )
    .unwrap();
    assert_eq!(image.pixel(1, 4), Some([0, 0, 0, 255]));
    assert_eq!(image.pixel(6, 4), Some([0, 0, 0, 0]));
}

#[rstest]
fn every_quality_and_instruction_set_draws_the_same_picture(
    #[values(Quality::Precise, Quality::Fast)] quality: Quality,
    #[values(Simd::Native, Simd::Portable)] simd: Simd,
) {
    let frame = Frame {
        background: Some(Color::WHITE),
        nodes: vec![Node::Layer(Layer {
            opacity: 0.7,
            transform: Affine::rotate(0.3),
            ..Layer::new(vec![cover(6.0, Color::rgba(0.2, 0.4, 0.9, 1.0))])
        })],
        ..Frame::default()
    };
    let golden = render_with(&frame, 8, 8, RenderOptions::GOLDEN).unwrap();
    let image = render_with(&frame, 8, 8, RenderOptions { quality, simd }).unwrap();
    // Integer and float pipelines may round a step apart, never more.
    let worst = image
        .pixels()
        .iter()
        .zip(golden.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max();
    assert!(worst <= Some(2), "{quality:?} {simd:?}: {worst:?}");
    assert_eq!(image.width(), 8);
}

#[test]
fn render_draws_with_the_default_options() {
    let frame = Frame {
        nodes: vec![cover(8.0, Color::BLACK)],
        ..Frame::default()
    };
    assert_eq!(
        render(&frame, 8, 8).unwrap(),
        render_with(&frame, 8, 8, RenderOptions::default()).unwrap()
    );
}

#[test]
fn a_glyph_the_font_does_not_hold_is_an_error() {
    let font = vello_cpu::peniko::FontData::new(
        vello_cpu::peniko::Blob::new(std::sync::Arc::new(
            include_bytes!("../fixtures/FixtureSans-Black.ttf").as_slice(),
        )),
        0,
    );
    let draw = crate::list::GlyphDraw {
        font,
        size: 12.0,
        glyphs: vec![crate::list::Glyph {
            id: 60_000,
            x: 0.0,
            y: 6.0,
        }],
        glyph_transform: None,
        coords: Vec::new(),
        paint: Paint::Solid(Color::BLACK),
        style: DrawStyle::Fill(FillRule::NonZero),
    };
    let frame = Frame {
        nodes: vec![Node::Layer(Layer {
            isolate: true,
            ..Layer::new(vec![Node::Glyphs(draw)])
        })],
        ..Frame::default()
    };
    let e = render(&frame, 8, 8).unwrap_err();
    assert_eq!(e.kind, Kind::Invalid);
    assert_eq!(e.expected.as_deref(), Some("glyph ids the font holds"));
}
