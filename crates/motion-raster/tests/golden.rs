//! Golden images: small display lists drawn with [`RenderOptions::GOLDEN`]
//! and compared byte for byte with the PNGs in `tests/golden/` ([D12]).
//!
//! Run with `MOTION_BLESS=1` to rewrite the PNGs after a deliberate change;
//! a failing comparison writes what it drew next to the target directory as
//! `<name>.actual.png`.
//!
//! [D12]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d12-what-repeatable-means

use std::path::PathBuf;
use std::sync::Arc;

use motion_raster::kurbo::{Affine, BezPath, Circle, Point, Rect, Shape};
use motion_raster::peniko::{Blob, FontData};
use motion_raster::{
    BLEND_MODES, Cap, Clip, Color, Dashes, DrawStyle, Effect, FillRule, Frame, Glyph, GlyphDraw,
    Image, ImageDraw, Join, Layer, Matte, MatteMode, Node, ParamValue, PathDraw, Picture,
    RenderOptions, StrokeStyle, render_with,
};

const FONT: &[u8] = include_bytes!("fixtures/FixtureSans-Black.ttf");

/// Draws `frame` and compares it with `tests/golden/{name}.png`.
fn golden(name: &str, frame: &Frame, width: u32, height: u32) {
    let image = render_with(frame, width, height, RenderOptions::GOLDEN).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"));
    if std::env::var_os("MOTION_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, image.to_png().unwrap()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("{} is missing; run with MOTION_BLESS=1", path.display()));
    let expected = Image::from_png(&bytes).unwrap();
    if expected != image {
        let actual = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        std::fs::write(&actual, image.to_png().unwrap()).unwrap();
        let (count, worst) = difference(&expected, &image);
        panic!(
            "{name}: {count} bytes differ, by up to {worst}; drawn: {}",
            actual.display()
        );
    }
}

/// How many bytes differ, and the largest difference.
fn difference(a: &Image, b: &Image) -> (usize, u8) {
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return (usize::MAX, u8::MAX);
    }
    a.pixels()
        .iter()
        .zip(b.pixels())
        .filter(|(x, y)| x != y)
        .fold((0, 0), |(n, worst), (x, y)| {
            (n + 1, worst.max(x.abs_diff(*y)))
        })
}

fn solid(r: f32, g: f32, b: f32, a: f32) -> motion_raster::Paint {
    motion_raster::Paint::Solid(Color::rgba(r, g, b, a))
}

fn fill(path: BezPath, paint: motion_raster::Paint) -> Node {
    Node::Path(PathDraw {
        path,
        paint,
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

fn frame(nodes: Vec<Node>) -> Frame {
    Frame {
        background: Some(Color::WHITE),
        nodes,
        ..Frame::default()
    }
}

#[test]
fn rect() {
    let square = Rect::new(16.0, 12.0, 48.0, 52.0).to_path(0.1);
    golden(
        "rect",
        &frame(vec![fill(square, solid(0.9, 0.1, 0.1, 1.0))]),
        64,
        64,
    );
}

#[test]
fn rotated_rect_about_its_anchor() {
    // A 40x20 box whose anchor, its own (10, 10), sits at (32, 32), turned 30° clockwise.
    let layer = Layer {
        transform: Affine::translate((32.0, 32.0))
            * Affine::rotate(30f64.to_radians())
            * Affine::translate((-10.0, -10.0)),
        ..Layer::new(vec![fill(
            Rect::new(0.0, 0.0, 40.0, 20.0).to_path(0.1),
            solid(0.1, 0.3, 0.9, 1.0),
        )])
    };
    golden("rotated_rect", &frame(vec![Node::Layer(layer)]), 64, 64);
}

/// An 8x4 picture: four columns of colour over two rows, the bottom-right
/// pixel half transparent.
fn tiles() -> Picture {
    let colours: [[u8; 4]; 4] = [
        [255, 196, 0, 255],
        [20, 120, 255, 255],
        [30, 200, 120, 255],
        [240, 60, 90, 255],
    ];
    let mut pixels = Vec::new();
    for y in 0..4 {
        for x in 0..8 {
            let mut colour = colours[x / 2];
            if y >= 2 {
                colour.swap(0, 2);
            }
            if (x, y) == (7, 3) {
                colour[3] = 128;
            }
            pixels.extend(colour);
        }
    }
    Picture::from_image(&Image::new(8, 4, pixels).unwrap()).unwrap()
}

#[test]
fn pictures_stretched_turned_and_faded() {
    // As drawn, then stretched into a 24x12 box at the corner, then the same
    // box turned 20° about its centre at 60% opacity over a bar, so the bar
    // shows through it.
    let draw = |rect: Rect| {
        Node::Image(ImageDraw {
            picture: tiles(),
            rect,
        })
    };
    let turned = Layer {
        transform: Affine::translate((38.0, 40.0))
            * Affine::rotate(20f64.to_radians())
            * Affine::translate((-12.0, -6.0)),
        opacity: 0.6,
        ..Layer::new(vec![draw(Rect::new(0.0, 0.0, 24.0, 12.0))])
    };
    let nodes = vec![
        draw(Rect::new(2.0, 2.0, 10.0, 6.0)),
        draw(Rect::new(14.0, 4.0, 38.0, 16.0)),
        fill(
            Rect::new(0.0, 36.0, 64.0, 44.0).to_path(0.1),
            solid(0.1, 0.1, 0.1, 1.0),
        ),
        Node::Layer(turned),
    ];
    golden("pictures", &frame(nodes), 64, 64);
}

#[test]
fn nested_clip_with_opacity() {
    let inner = Layer {
        clip: Some(Clip::Rect(Rect::new(8.0, 8.0, 40.0, 40.0))),
        ..Layer::new(vec![fill(
            Circle::new((40.0, 40.0), 28.0).to_path(0.1),
            solid(0.0, 0.6, 0.3, 1.0),
        )])
    };
    let outer = Layer {
        transform: Affine::translate((8.0, 8.0)),
        opacity: 0.6,
        ..Layer::new(vec![
            fill(
                Rect::new(0.0, 0.0, 48.0, 48.0).to_path(0.1),
                solid(1.0, 0.8, 0.0, 1.0),
            ),
            Node::Layer(inner),
        ])
    };
    golden("nested_clip", &frame(vec![Node::Layer(outer)]), 64, 64);
}

#[test]
fn open_stroke_with_round_caps_and_dashes() {
    let mut chevron = BezPath::new();
    chevron.move_to((12.0, 40.0));
    chevron.line_to((32.0, 20.0));
    chevron.line_to((52.0, 40.0));
    let mut line = BezPath::new();
    line.move_to((8.0, 54.0));
    line.line_to((56.0, 54.0));
    let round = StrokeStyle {
        cap: Cap::Round,
        join: Join::Round,
        ..StrokeStyle::new(8.0)
    };
    let dashed = StrokeStyle {
        dashes: Some(Dashes {
            offset: 0.0,
            pattern: vec![6.0, 4.0],
        }),
        ..StrokeStyle::new(3.0)
    };
    let nodes = vec![
        Node::Path(PathDraw {
            path: chevron,
            paint: solid(0.0, 0.0, 0.0, 1.0),
            style: DrawStyle::Stroke(round),
        }),
        Node::Path(PathDraw {
            path: line,
            paint: solid(0.8, 0.0, 0.4, 1.0),
            style: DrawStyle::Stroke(dashed),
        }),
    ];
    golden("stroke", &frame(nodes), 64, 64);
}

#[test]
fn fill_rules() {
    // Two squares, one inside the other: even-odd leaves a hole, non-zero does not.
    let mut rings = Rect::new(4.0, 4.0, 28.0, 28.0).to_path(0.1);
    rings.extend(Rect::new(10.0, 10.0, 22.0, 22.0).to_path(0.1));
    let shape = |rule, dx: f64| {
        let mut path = rings.clone();
        path.apply_affine(Affine::translate((dx, 0.0)));
        Node::Path(PathDraw {
            path,
            paint: solid(0.2, 0.2, 0.2, 1.0),
            style: DrawStyle::Fill(rule),
        })
    };
    golden(
        "fill_rules",
        &frame(vec![
            shape(FillRule::NonZero, 0.0),
            shape(FillRule::EvenOdd, 32.0),
        ]),
        64,
        32,
    );
}

#[test]
fn fill_effect_recolours_a_group() {
    // A red square and a half-transparent blue circle, filled white in full
    // and then at half strength on a dark background.
    let content = || {
        vec![
            fill(
                Rect::new(4.0, 4.0, 24.0, 24.0).to_path(0.1),
                solid(1.0, 0.0, 0.0, 1.0),
            ),
            fill(
                Circle::new((22.0, 22.0), 9.0).to_path(0.1),
                solid(0.0, 0.0, 1.0, 0.5),
            ),
        ]
    };
    let white = |opacity: f32, dx: f64| {
        let effect = Effect::new(
            "fill",
            &[
                ("color", ParamValue::Color(Color::WHITE)),
                ("opacity", ParamValue::Number(opacity)),
            ],
        )
        .unwrap();
        Node::Layer(Layer {
            transform: Affine::translate((dx, 0.0)),
            effects: vec![effect],
            ..Layer::new(content())
        })
    };
    let frame = Frame {
        background: Some(Color::rgba(0.1, 0.1, 0.15, 1.0)),
        ..frame(vec![white(1.0, 0.0), white(0.5, 32.0)])
    };
    golden("fill_effect", &frame, 64, 36);
}

#[test]
fn every_matte_mode() {
    // One 32x32 cell per mode, each moved by its own transform: an orange bar
    // the matte leaves alone, and a blue square matted by a white circle over
    // a half-grey band, half of the circle at half opacity.
    let modes = [
        MatteMode::Alpha,
        MatteMode::AlphaInverted,
        MatteMode::Luma,
        MatteMode::LumaInverted,
    ];
    let cells: Vec<Node> = modes
        .iter()
        .enumerate()
        .map(|(i, mode)| {
            let source = Layer::new(vec![
                fill(
                    Rect::new(2.0, 22.0, 30.0, 30.0).to_path(0.1),
                    solid(0.5, 0.5, 0.5, 1.0),
                ),
                fill(
                    Circle::new((16.0, 14.0), 9.0).to_path(0.1),
                    solid(1.0, 1.0, 1.0, 1.0),
                ),
                Node::Layer(Layer {
                    opacity: 0.5,
                    ..Layer::new(vec![fill(
                        Rect::new(16.0, 2.0, 30.0, 20.0).to_path(0.1),
                        solid(1.0, 1.0, 1.0, 1.0),
                    )])
                }),
            ]);
            let matted = Layer {
                matte: Some(Matte {
                    source: Box::new(Node::Layer(source)),
                    mode: *mode,
                }),
                ..Layer::new(vec![fill(
                    Rect::new(2.0, 2.0, 30.0, 30.0).to_path(0.1),
                    solid(0.1, 0.3, 0.9, 1.0),
                )])
            };
            Node::Layer(Layer {
                transform: Affine::translate((i as f64 * 32.0, 0.0)),
                ..Layer::new(vec![
                    fill(
                        Rect::new(0.0, 12.0, 32.0, 18.0).to_path(0.1),
                        solid(1.0, 0.5, 0.1, 1.0),
                    ),
                    Node::Layer(matted),
                ])
            })
        })
        .collect();
    golden("matte_modes", &frame(cells), 128, 32);
}

#[test]
fn every_blend_mode() {
    // One 16x16 cell per mode: an orange square below, a blue-green circle
    // above it in the mode, five cells to a row.
    let cells: Vec<Node> = BLEND_MODES
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            let (col, row) = ((i % 5) as f64, (i / 5) as f64);
            let top = Layer {
                blend: spec.mode,
                ..Layer::new(vec![fill(
                    Circle::new((10.0, 10.0), 6.0).to_path(0.1),
                    solid(0.1, 0.6, 0.7, 0.9),
                )])
            };
            Node::Layer(Layer {
                transform: Affine::translate((col * 16.0, row * 16.0)),
                // Each cell blends only with its own square.
                isolate: true,
                ..Layer::new(vec![
                    fill(
                        Rect::new(2.0, 2.0, 12.0, 12.0).to_path(0.1),
                        solid(1.0, 0.5, 0.1, 1.0),
                    ),
                    Node::Layer(top),
                ])
            })
        })
        .collect();
    let rows = BLEND_MODES.len().div_ceil(5) as u32;
    golden("blend_modes", &frame(cells), 80, rows * 16);
}

#[test]
fn glyph_run_filled_and_stroked() {
    let font = FontData::new(Blob::new(Arc::new(FONT)), 0);
    // "Hi #mixkit": glyph ids and advances from the font, in units of 2000 per em.
    let ids_and_advances = [
        (9, 0),
        (55, 1547),
        (1, 2067),
        (109, 2542),
        (59, 3702),
        (55, 5389),
        (76, 5909),
        (58, 6973),
        (55, 8064),
        (67, 8584),
    ];
    let run = |size: f32, y: f32, style: DrawStyle, paint| {
        Node::Glyphs(GlyphDraw {
            font: font.clone(),
            size,
            glyphs: ids_and_advances
                .iter()
                .map(|&(id, advance)| Glyph {
                    id,
                    x: 4.0 + advance as f32 * size / 2000.0,
                    y,
                })
                .collect(),
            glyph_transform: None,
            coords: Vec::new(),
            paint,
            style,
        })
    };
    let slanted = match run(
        14.0,
        50.0,
        DrawStyle::Fill(FillRule::NonZero),
        solid(0.8, 0.1, 0.1, 1.0),
    ) {
        Node::Glyphs(draw) => Node::Glyphs(GlyphDraw {
            glyph_transform: Some(Affine::skew(-0.25, 0.0)),
            ..draw
        }),
        _ => unreachable!(),
    };
    let nodes = vec![
        run(
            16.0,
            18.0,
            DrawStyle::Fill(FillRule::NonZero),
            solid(0.0, 0.0, 0.0, 1.0),
        ),
        run(
            16.0,
            36.0,
            DrawStyle::Stroke(StrokeStyle::new(1.0)),
            solid(0.1, 0.2, 0.8, 1.0),
        ),
        slanted,
    ];
    golden("glyphs", &frame(nodes), 96, 56);
}

#[test]
fn a_transparent_background_stays_transparent() {
    let frame = Frame {
        nodes: vec![fill(
            Circle::new(Point::new(16.0, 16.0), 10.0).to_path(0.1),
            solid(0.0, 0.5, 1.0, 0.5),
        )],
        ..Frame::default()
    };
    golden("transparent", &frame, 32, 32);
}
