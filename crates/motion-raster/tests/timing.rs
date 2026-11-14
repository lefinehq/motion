//! How long a story-sized frame takes: run with
//! `cargo test --release -p motion-raster --test timing -- --ignored --nocapture`.
//! Ignored by default, since debug builds and shared CI machines time nothing useful.

use std::sync::Arc;
use std::time::Instant;

use motion_raster::kurbo::{Affine, BezPath, Circle, Rect, Shape};
use motion_raster::peniko::{Blob, FontData};
use motion_raster::{
    Cap, Clip, Color, DrawStyle, Effect, FillRule, Frame, Glyph, GlyphDraw, Join, Layer, Node,
    Paint, ParamValue, PathDraw, Quality, RenderOptions, StrokeStyle, render_with,
};

const FONT: &[u8] = include_bytes!("fixtures/FixtureSans-Black.ttf");

fn fill(path: BezPath, color: Color) -> Node {
    Node::Path(PathDraw {
        path,
        paint: Paint::Solid(color),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// About ten layers like a story template's: a full-frame plate, two large
/// rotated squares, a clipped nested group, three text lines, a stroked arrow
/// and two recoloured groups.
fn story() -> Frame {
    let font = FontData::new(Blob::new(Arc::new(FONT)), 0);
    let text = |y: f32| {
        Node::Glyphs(GlyphDraw {
            font: font.clone(),
            size: 72.0,
            glyphs: (0..12)
                .map(|i| Glyph {
                    id: 9 + i,
                    x: 300.0 + i as f32 * 46.0,
                    y,
                })
                .collect(),
            glyph_transform: None,
            coords: Vec::new(),
            paint: Paint::Solid(Color::BLACK),
            style: DrawStyle::Fill(FillRule::NonZero),
        })
    };
    let square = |side: f64, at: (f64, f64), color| {
        Node::Layer(Layer {
            transform: Affine::translate(at) * Affine::rotate(45f64.to_radians()),
            ..Layer::new(vec![fill(
                Rect::new(-side / 2.0, -side / 2.0, side / 2.0, side / 2.0).to_path(0.1),
                color,
            )])
        })
    };
    let mut arrow = BezPath::new();
    arrow.move_to((480.0, 1840.0));
    arrow.line_to((540.0, 1790.0));
    arrow.line_to((600.0, 1840.0));
    let recoloured = |y: f64| {
        Node::Layer(Layer {
            effects: vec![
                Effect::new("fill", &[("color", ParamValue::Color(Color::BLACK))]).unwrap(),
            ],
            ..Layer::new(vec![fill(
                Circle::new((540.0, y), 40.0).to_path(0.1),
                Color::WHITE,
            )])
        })
    };
    Frame {
        background: Some(Color::BLACK),
        nodes: vec![
            fill(
                Rect::new(0.0, 0.0, 1080.0, 1920.0).to_path(0.1),
                Color::rgba(0.3, 0.3, 0.3, 1.0),
            ),
            square(1126.0, (540.0, 1500.0), Color::rgba(1.0, 0.88, 0.0, 1.0)),
            square(500.0, (540.0, 1120.0), Color::WHITE),
            Node::Layer(Layer {
                clip: Some(Clip::Rect(Rect::new(0.0, 1500.0, 1080.0, 1900.0))),
                opacity: 0.9,
                ..Layer::new(vec![text(1640.0)])
            }),
            text(1000.0),
            text(1090.0),
            text(1180.0),
            Node::Path(PathDraw {
                path: arrow,
                paint: Paint::Solid(Color::BLACK),
                style: DrawStyle::Stroke(StrokeStyle {
                    cap: Cap::Round,
                    join: Join::Round,
                    ..StrokeStyle::new(8.0)
                }),
            }),
            recoloured(300.0),
            recoloured(500.0),
        ],
        ..Frame::default()
    }
}

#[test]
#[ignore = "a timing, not a check"]
fn story_frame() {
    let frame = story();
    for (name, options) in [
        ("native", RenderOptions::default()),
        (
            "fast",
            RenderOptions {
                quality: Quality::Fast,
                ..RenderOptions::default()
            },
        ),
        ("golden", RenderOptions::GOLDEN),
    ] {
        render_with(&frame, 1080, 1920, options).unwrap();
        let runs = 10;
        let start = Instant::now();
        for _ in 0..runs {
            render_with(&frame, 1080, 1920, options).unwrap();
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
        println!("{name}: {ms:.1} ms per 1080x1920 frame");
    }
}
