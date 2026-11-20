//! How long a story-sized frame takes: run with
//! `cargo test --release -p nakkash-raster --test timing -- --ignored --nocapture`.
//! Ignored by default, since debug builds and shared CI machines time nothing useful.

use std::sync::Arc;
use std::time::Instant;

use nakkash_raster::kurbo::{Affine, BezPath, Circle, Rect, Shape};
use nakkash_raster::peniko::{Blob, FontData};
use nakkash_raster::{
    Cap, Clip, Color, DrawStyle, Effect, FillRule, Frame, Glyph, GlyphDraw, Homography, Join,
    Layer, Node, Paint, ParamValue, PathDraw, Pixels, Projection, Quality, RenderOptions, Space,
    StrokeStyle, render_with,
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

/// A 1920x1080 card, a plate with circles, a bar and a line of text,
/// as a 1080p frame draws it: flat, or turned 35° about the vertical
/// through the default camera (a zoom of 2666.7 px), so it is drawn on its
/// own plane and laid onto the frame through the projection.
fn card(projected: bool) -> Frame {
    let font = FontData::new(Blob::new(Arc::new(FONT)), 0);
    let (w, h) = (1920.0, 1080.0);
    let mut nodes = vec![fill(
        Rect::new(0.0, 0.0, w, h).to_path(0.1),
        Color::rgba(0.95, 0.9, 0.8, 1.0),
    )];
    for i in 0..6 {
        nodes.push(fill(
            Circle::new((200.0 + f64::from(i) * 300.0, 400.0), 120.0).to_path(0.1),
            Color::rgba(0.1 * f64::from(i) as f32, 0.3, 0.6, 1.0),
        ));
    }
    nodes.push(fill(
        Rect::new(100.0, 700.0, 1820.0, 760.0).to_path(0.1),
        Color::BLACK,
    ));
    nodes.push(Node::Glyphs(GlyphDraw {
        font,
        size: 120.0,
        glyphs: (0..14)
            .map(|i| Glyph {
                id: 9 + i,
                x: 160.0 + i as f32 * 110.0,
                y: 950.0,
            })
            .collect(),
        glyph_transform: None,
        coords: Vec::new(),
        paint: Paint::Solid(Color::BLACK),
        style: DrawStyle::Fill(FillRule::NonZero),
    }));
    let (zoom, angle) = (2666.7, 35f64.to_radians());
    let (s, c) = angle.sin_cos();
    // The card's centre at the frame's, its x turned into depth.
    let depth = [s / zoom, 0.0, 1.0 - w / 2.0 * s / zoom];
    let row = |at: f64, own: [f64; 3]| std::array::from_fn(|i| at * depth[i] + own[i]);
    let map = Homography::new([
        row(w / 2.0, [c, 0.0, -w / 2.0 * c]),
        row(h / 2.0, [0.0, 1.0, -h / 2.0]),
        depth,
    ]);
    let layer = Layer {
        bounds: Some(Rect::new(0.0, 0.0, w, h)),
        isolate: true,
        projection: projected.then(|| Projection::new(map)),
        ..Layer::new(nodes)
    };
    Frame {
        background: Some(Color::BLACK),
        nodes: vec![Node::Layer(layer)],
        ..Frame::default()
    }
}

/// The median of `runs` timings of drawing `frame`, in ms.
fn median_ms(frame: &Frame, width: u32, height: u32, options: RenderOptions, runs: usize) -> f64 {
    render_with(frame, width, height, options).unwrap();
    let mut times: Vec<f64> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            render_with(frame, width, height, options).unwrap();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[runs / 2]
}

#[test]
#[ignore = "a timing, not a check"]
fn projected_card() {
    for projected in [false, true] {
        let frame = card(projected);
        let name = if projected { "projected" } else { "flat" };
        for (quality, options) in [
            ("native", RenderOptions::default()),
            (
                "fast",
                RenderOptions {
                    quality: Quality::Fast,
                    ..RenderOptions::default()
                },
            ),
        ] {
            let ms = median_ms(&frame, 1920, 1080, options, 21);
            println!("{name} card, {quality}: {ms:.1} ms median per 1920x1080 frame");
        }
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

/// A `width` by `height` grid of varied, partly transparent colours.
fn textured(width: u16, height: u16) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    let w = usize::from(width);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = ((i % w) as f32, (i / w) as f32);
        let a = (0.6 + 0.4 * (x * 0.013).sin() * (y * 0.017).cos()).clamp(0.0, 1.0);
        let c = |k: f32| (0.5 + 0.5 * (x * k + y * (0.02 - k)).sin()) * a;
        *p = [c(0.007), c(0.011), c(0.003), a];
    }
    pixels
}

/// An effect to time: what to print, its name and its parameters.
type Timed = (&'static str, &'static str, Vec<(&'static str, ParamValue)>);

/// The median of `runs` timings of `effect` on `pixels`, on a pool of
/// `threads`, in ms, and what it drew.
fn effect_ms(effect: &Effect, pixels: &Pixels, threads: usize, runs: usize) -> (f64, Pixels) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    let space = Space {
        transform: Affine::IDENTITY,
        bounds: None,
        input: None,
        time: 0.5,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
    };
    pool.install(|| {
        let mut drawn = pixels.clone();
        let mut times: Vec<f64> = (0..runs)
            .map(|_| {
                drawn = pixels.clone();
                let start = Instant::now();
                effect.apply(&mut drawn, &space);
                start.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        times.sort_by(f64::total_cmp);
        (times[runs / 2], drawn)
    })
}

#[test]
#[ignore = "a timing, not a check"]
fn effects_on_one_thread_and_on_all() {
    let all = std::thread::available_parallelism().map_or(1, usize::from);
    let number = |v: f32| ParamValue::Number(v);
    let effects: [Timed; 10] = [
        (
            "gaussian-blur 20",
            "gaussian-blur",
            vec![("blurriness", number(20.0))],
        ),
        (
            "box-blur r10 x3",
            "box-blur",
            vec![("radius", number(10.0)), ("iterations", number(3.0))],
        ),
        (
            "glow r20",
            "glow",
            vec![("radius", number(20.0)), ("threshold", number(0.3))],
        ),
        (
            "fractal-noise",
            "fractal-noise",
            vec![("complexity", number(6.0))],
        ),
        (
            "turbulent-displace",
            "turbulent-displace",
            vec![("amount", number(50.0))],
        ),
        (
            "displacement-map",
            "displacement-map",
            vec![("horizontal", number(20.0))],
        ),
        (
            "drop-shadow",
            "drop-shadow",
            vec![("softness", number(30.0))],
        ),
        ("sharpen", "sharpen", vec![("amount", number(50.0))]),
        (
            "hue-saturation",
            "hue-saturation",
            vec![("hue", number(40.0))],
        ),
        (
            "polar-coordinates",
            "polar-coordinates",
            vec![("interpolation", number(1.0))],
        ),
    ];
    for (width, height) in [(1920, 1080), (3840, 2160)] {
        let pixels = textured(width, height);
        for (what, name, params) in &effects {
            let effect = Effect::new(name, params).unwrap();
            let (one, serial) = effect_ms(&effect, &pixels, 1, 3);
            let mut line = format!("{width}x{height} {what}: {one:.1} ms on 1 thread");
            let mut counts = vec![2, 4, all];
            counts.retain(|&n| n > 1 && n <= all);
            counts.dedup();
            for threads in counts {
                let (ms, drawn) = effect_ms(&effect, &pixels, threads, 5);
                assert!(
                    drawn == serial,
                    "{what} differs between 1 and {threads} threads"
                );
                line += &format!(", {ms:.1} on {threads} ({:.1}x)", one / ms);
            }
            println!("{line}, the same bits on each");
        }
    }
}
