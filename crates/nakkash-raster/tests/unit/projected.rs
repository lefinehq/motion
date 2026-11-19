//! Unit tests for `src/projected.rs`: projected layers drawn through
//! `render_with` and measured against the flat path, against geometry
//! worked out by hand, and against supersampled references.

use super::*;
use crate::effect::{EFFECTS, ParamValue};
use crate::image::Image;
use crate::list::{Frame, LayerMask, MaskMode, PathDraw, Projection};
use crate::render::{RenderOptions, render_with};
use crate::style::{Color, FillRule, Paint};
use nakkash_format::Kind;
use rstest::rstest;
use std::sync::Arc;
use vello_cpu::kurbo::{BezPath, Circle};
use vello_cpu::peniko::{Blob, FontData};

const FONT: &[u8] = include_bytes!("../fixtures/FixtureSans-Black.ttf");

/// A filled outline in `color`.
fn fill(path: BezPath, color: Color) -> Node {
    Node::Path(PathDraw {
        path,
        paint: Paint::Solid(color),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// A card: a white `w` by `h` rect from the origin, a black circle and a
/// red bar on it, so edges run every way.
fn card(w: f64, h: f64) -> Layer {
    Layer {
        bounds: Some(Rect::new(0.0, 0.0, w, h)),
        ..Layer::new(vec![
            fill(Rect::new(0.0, 0.0, w, h).to_path(0.1), Color::WHITE),
            fill(
                Circle::new((w * 0.35, h * 0.5), h * 0.3).to_path(0.1),
                Color::BLACK,
            ),
            fill(
                Rect::new(w * 0.6, h * 0.2, w * 0.7, h * 0.8).to_path(0.1),
                Color::rgba(1.0, 0.0, 0.0, 1.0),
            ),
        ])
    }
}

/// The map of a plane through a camera `zoom` px in front of the canvas
/// centred at `centre`: the plane's point `anchor` sits at the centre,
/// turned `y` degrees about the vertical and `x` about the horizontal,
/// and pushed `z` px away.
fn camera(zoom: f64, centre: Point, anchor: Point, y: f64, x: f64, z: f64) -> Homography {
    let (sy, cy) = y.to_radians().sin_cos();
    let (sx, cx) = x.to_radians().sin_cos();
    // World offsets from the centre, as linear functions of (u, v, 1):
    // turned about Y, then about X.
    let ox = [cy, 0.0, -anchor.x * cy];
    let oy0 = [0.0, 1.0, -anchor.y];
    let oz0 = [sy, 0.0, -anchor.x * sy];
    let oy: [f64; 3] = std::array::from_fn(|i| oy0[i] * cx - oz0[i] * sx);
    let mut oz: [f64; 3] = std::array::from_fn(|i| oy0[i] * sx + oz0[i] * cx);
    oz[2] += z;
    // w is depth over the zoom; x' and y' put the centre back at w's scale.
    let w: [f64; 3] = std::array::from_fn(|i| oz[i] / zoom + if i == 2 { 1.0 } else { 0.0 });
    let row = |c: f64, o: [f64; 3]| std::array::from_fn(|i| c * w[i] + o[i]);
    Homography::new([row(centre.x, ox), row(centre.y, oy), w])
}

/// `layer` projected through `map`.
fn projected(layer: Layer, map: Homography) -> Node {
    Node::Layer(Layer {
        projection: Some(Projection::new(map)),
        ..layer
    })
}

/// `nodes` drawn on a `w` by `h` canvas with golden options.
fn draw(nodes: Vec<Node>, w: u32, h: u32) -> Image {
    let frame = Frame {
        nodes,
        ..Frame::default()
    };
    render_with(&frame, w, h, RenderOptions::GOLDEN).unwrap()
}

/// The largest and the mean difference of two images' channels, each
/// premultiplied by its alpha, so a faint px's colour weighs as little as
/// it shows.
fn difference(a: &Image, b: &Image) -> (u8, f64) {
    let premultiplied = |p: &[u8]| -> [u8; 4] {
        let pre = |c: u8| ((u16::from(c) * u16::from(p[3]) + 127) / 255) as u8;
        [pre(p[0]), pre(p[1]), pre(p[2]), p[3]]
    };
    let (worst, sum) = a
        .pixels()
        .chunks(4)
        .zip(b.pixels().chunks(4))
        .flat_map(|(x, y)| {
            let (x, y) = (premultiplied(x), premultiplied(y));
            (0..4).map(move |c| x[c].abs_diff(y[c]))
        })
        .fold((0u8, 0u64), |(w, s), d| (w.max(d), s + u64::from(d)));
    (worst, sum as f64 / a.pixels().len() as f64)
}

/// The alpha of each px of row `y`, 0 to 1.
fn row_alpha(image: &Image, y: u32) -> Vec<f64> {
    (0..image.width())
        .map(|x| f64::from(image.pixel(x, y).unwrap()[3]) / 255.0)
        .collect()
}

#[rstest]
#[case::identity(Affine::IDENTITY, 1, 0.01)]
#[case::whole_px_move(Affine::translate((7.0, -3.0)), 1, 0.01)]
#[case::turned_and_scaled(
    Affine::translate((90.0, 10.0)) * Affine::rotate(0.5) * Affine::scale(1.3),
    64,
    0.5
)]
fn an_affine_projection_draws_as_the_flat_path(
    #[case] transform: Affine,
    #[case] worst: u8,
    #[case] mean: f64,
) {
    let flat = draw(
        vec![Node::Layer(Layer {
            transform,
            isolate: true,
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let laid = draw(
        vec![projected(card(120.0, 80.0), Homography::from(transform))],
        200,
        160,
    );
    let (w, m) = difference(&flat, &laid);
    assert!(w <= worst && m <= mean, "worst {w}, mean {m:.3}");
}

#[rstest]
#[case::painted_over(Layer {
    effects: vec![Effect::new("fill", &[("color", ParamValue::Color(Color::rgba(0.0, 0.5, 1.0, 1.0)))]).unwrap()],
    ..card(120.0, 80.0)
})]
#[case::clipped(Layer {
    clip: Some(Clip::Path(Circle::new((60.0, 40.0), 30.0).to_path(0.1), FillRule::NonZero)),
    ..card(120.0, 80.0)
})]
#[case::moved_then_tinted(Layer {
    effects: vec![
        Effect::new("transform", &[("position", ParamValue::Point([30.0, 10.0]))]).unwrap(),
        Effect::new("invert", &[]).unwrap(),
    ],
    ..card(120.0, 80.0)
})]
fn effects_and_clips_on_the_buffer_draw_as_the_flat_path(#[case] layer: Layer) {
    let at = Affine::translate((20.0, 30.0));
    let flat = draw(
        vec![Node::Layer(Layer {
            transform: at,
            isolate: true,
            ..layer.clone()
        })],
        200,
        160,
    );
    let laid = draw(vec![projected(layer, Homography::from(at))], 200, 160);
    let (worst, mean) = difference(&flat, &laid);
    assert!(worst <= 1 && mean < 0.01, "worst {worst}, mean {mean:.3}");
}

#[test]
fn a_card_turned_sixty_degrees_is_foreshortened_by_its_depth() {
    let (zoom, centre) = (2666.7, Point::new(480.0, 270.0));
    let map = camera(zoom, centre, Point::new(200.0, 50.0), 60.0, 0.0, 0.0);
    let image = draw(vec![projected(card(400.0, 100.0), map)], 960, 540);
    let alpha = row_alpha(&image, 270);
    // Half the card each side of its anchor: 100 px across and 173.2 in
    // depth, seen at zoom / (zoom ± 173.2).
    let depth = 200.0 * 60f64.to_radians().sin();
    let left = 100.0 * zoom / (zoom - depth);
    let right = 100.0 * zoom / (zoom + depth);
    let covered: f64 = alpha.iter().sum();
    assert!(
        (covered - (left + right)).abs() < 0.5,
        "{covered:.2} px across, expected {:.2}",
        left + right
    );
    // The first px from each side that is half covered.
    let near = alpha.iter().position(|a| *a >= 0.5).unwrap() as f64;
    let far = alpha.iter().rposition(|a| *a >= 0.5).unwrap() as f64 + 1.0;
    assert!((centre.x - near - left).abs() < 1.0, "left edge at {near}");
    assert!((far - centre.x - right).abs() < 1.0, "right edge at {far}");
}

#[test]
fn a_plane_crossing_the_camera_draws_only_what_is_in_front() {
    // Turned 80° about the vertical, 200 px from a camera of zoom 200: the
    // plane's left side, from 203 px left of its anchor, is behind it.
    let card = Layer {
        bounds: Some(Rect::new(0.0, 0.0, 800.0, 800.0)),
        ..Layer::new(vec![fill(
            Rect::new(0.0, 0.0, 800.0, 800.0).to_path(0.1),
            Color::WHITE,
        )])
    };
    let map = camera(
        200.0,
        Point::new(160.0, 90.0),
        Point::new(400.0, 400.0),
        80.0,
        0.0,
        0.0,
    );
    let image = draw(vec![projected(card, map)], 320, 180);
    let alpha = row_alpha(&image, 90);
    // The front part runs out to infinity on the left and ends at its
    // right edge, 23.4 px right of the centre; drawn without the cut, the
    // part behind would land from 71.6 px right of it onwards.
    let edge =
        160.0 + 400.0 * 80f64.to_radians().cos() / (1.0 + 400.0 * 80f64.to_radians().sin() / 200.0);
    assert!((edge - 183.4).abs() < 0.1);
    for (x, a) in alpha.iter().enumerate() {
        let x = x as f64 + 0.5;
        if x < edge - 1.0 {
            assert!(*a > 0.99, "px {x} is {a}");
        }
        if x > edge + 1.0 {
            assert!(*a < 0.01, "px {x} is {a}");
        }
    }
    // Every row of the left side is covered: the plane fills the height.
    assert!((0..180).all(|y| image.pixel(10, y).unwrap()[3] == 255));
}

/// The colour of a checkerboard of `cell`-unit squares at `(u, v)`,
/// white and black, or `None` off its `side`-unit square.
fn checker(u: f64, v: f64, cell: f64, side: f64) -> Option<f64> {
    if !(0.0..side).contains(&u) || !(0.0..side).contains(&v) {
        return None;
    }
    let parity = ((u / cell).floor() + (v / cell).floor()) as i64 % 2;
    Some(if parity == 0 { 1.0 } else { 0.0 })
}

#[test]
fn a_far_plane_seen_nearly_edge_on_does_not_alias() {
    let (cell, side) = (8.0, 2048.0);
    let mut black = BezPath::new();
    let cells = (side / cell) as usize;
    for i in 0..cells {
        for j in 0..cells {
            if (i + j) % 2 == 1 {
                let (u, v) = (i as f64 * cell, j as f64 * cell);
                black.extend(Rect::new(u, v, u + cell, v + cell).path_elements(0.1));
            }
        }
    }
    let floor = Layer {
        bounds: Some(Rect::new(0.0, 0.0, side, side)),
        ..Layer::new(vec![
            fill(Rect::new(0.0, 0.0, side, side).to_path(0.1), Color::WHITE),
            fill(black, Color::BLACK),
        ])
    };
    // Tipped 84° back about the horizontal and pushed away: its far end
    // squeezes many cells into each px.
    let (w, h) = (192u32, 108u32);
    let map = camera(
        300.0,
        Point::new(96.0, 54.0),
        Point::new(side / 2.0, side / 2.0),
        0.0,
        84.0,
        1500.0,
    );
    let image = draw(vec![projected(floor, map)], w, h);
    // The grey each px should have: the board's coverage averaged over
    // 16 by 16 points of it, black where it is clear.
    let back = map.inverse().unwrap();
    let n = 16;
    let (mut ours, mut point, mut count) = (0.0, 0.0, 0);
    for y in 0..h {
        for x in 0..w {
            let at = |sx: f64, sy: f64| {
                let [u, v, q] = back.map(Point::new(f64::from(x) + sx, f64::from(y) + sy));
                match q > 0.0 {
                    true => checker(u / q, v / q, cell, side),
                    false => None,
                }
            };
            let mut sum = 0.0;
            let mut seen = 0;
            for i in 0..n {
                for j in 0..n {
                    let s = (f64::from(i) + 0.5) / f64::from(n);
                    let t = (f64::from(j) + 0.5) / f64::from(n);
                    if let Some(c) = at(s, t) {
                        sum += c;
                        seen += 1;
                    }
                }
            }
            if seen < n * n {
                // Edges and sky: what the reference cannot weigh alike.
                continue;
            }
            let reference = sum / f64::from(n * n);
            let drawn = image.pixel(x, y).unwrap();
            let grey = f64::from(drawn[0]) / 255.0;
            ours += (grey - reference).abs();
            point += (at(0.5, 0.5).unwrap() - reference).abs();
            count += 1;
        }
    }
    let (ours, point) = (ours / f64::from(count), point / f64::from(count));
    assert!(count > 5_000, "{count} px compared");
    // Point sampling, as a renderer without the chain draws it, is off by
    // about half the way from black to white where cells crowd (0.49 when
    // measured); ours stays close (0.032, and 0.059 with one probe a px).
    assert!(
        ours < 0.045,
        "mean error {ours:.4}, point sampling {point:.4}"
    );
    assert!(
        ours * 8.0 < point,
        "mean error {ours:.4}, point sampling {point:.4}"
    );
}

/// The card with a feathered circular mask and a blur.
fn masked_blurred() -> Layer {
    let mut mask = Circle::new((60.0, 40.0), 34.0).to_path(0.1);
    mask.close_path();
    Layer {
        masks: vec![LayerMask {
            path: mask,
            mode: MaskMode::Add,
            inverted: false,
            opacity: 1.0,
            feather: [4.0, 4.0],
            expand: 0.0,
        }],
        effects: vec![
            Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(10.0))]).unwrap(),
        ],
        ..card(120.0, 80.0)
    }
}

#[test]
fn a_mask_and_a_blur_run_in_the_layers_own_units() {
    // Scaled up twice by the projection: the blur and the feather grow
    // with it, as the flat path's do under a transform.
    let scale = Affine::translate((20.0, 10.0)) * Affine::scale(2.0);
    let flat = draw(
        vec![Node::Layer(Layer {
            transform: scale,
            ..masked_blurred()
        })],
        280,
        180,
    );
    let laid = draw(
        vec![projected(masked_blurred(), Homography::from(scale))],
        280,
        180,
    );
    let (worst, mean) = difference(&flat, &laid);
    assert!(worst <= 2 && mean < 0.01, "worst {worst}, mean {mean:.3}");
}

#[test]
fn a_masked_blurred_plane_in_perspective_matches_a_supersampled_reference() {
    let map = camera(
        400.0,
        Point::new(100.0, 60.0),
        Point::new(60.0, 40.0),
        50.0,
        0.0,
        0.0,
    );
    let laid = draw(vec![projected(masked_blurred(), map)], 200, 120);
    // The layer drawn flat at 4× its size, then laid by hand: each px the
    // mean of 4 by 4 points mapped back onto it.
    let k = 4.0;
    let flat = draw(
        vec![Node::Layer(Layer {
            transform: Affine::translate((80.0, 80.0)) * Affine::scale(k),
            ..masked_blurred()
        })],
        640,
        480,
    );
    // Its alpha alone, which is what is compared.
    let mut alphas = Pixels::transparent(640, 480);
    for (px, rgba) in alphas.data_mut().iter_mut().zip(flat.pixels().chunks(4)) {
        px[3] = f32::from(rgba[3]) / 255.0;
    }
    let flat = alphas;
    let back = map.inverse().unwrap();
    let (mut sum, mut count) = (0.0, 0);
    for y in 0..120u32 {
        for x in 0..200u32 {
            let mut alpha = 0.0;
            for i in 0..4 {
                for j in 0..4 {
                    let p = Point::new(
                        f64::from(x) + (f64::from(i) + 0.5) / 4.0,
                        f64::from(y) + (f64::from(j) + 0.5) / 4.0,
                    );
                    let [u, v, q] = back.map(p);
                    let at = Point::new(80.0 + u / q * k, 80.0 + v / q * k);
                    alpha += f64::from(flat.sample(at.x, at.y)[3]) / 16.0;
                }
            }
            let drawn = f64::from(laid.pixel(x, y).unwrap()[3]) / 255.0;
            sum += (drawn - alpha).abs();
            count += 1;
        }
    }
    let mean = sum / f64::from(count);
    assert!(mean < 0.002, "mean alpha error {mean:.4}");
}

#[test]
fn motion_blur_samples_average_as_the_flat_moves_do() {
    let moves = [
        Affine::translate((-12.0, 0.0)),
        Affine::translate((12.0, 4.0)),
    ];
    let at = Affine::translate((40.0, 40.0));
    let flat = draw(
        vec![Node::Layer(Layer {
            transform: at,
            motion: moves.to_vec(),
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let laid = draw(
        vec![Node::Layer(Layer {
            projection: Some(Projection {
                matrix: Homography::from(at),
                samples: moves.iter().map(|m| Homography::from(*m * at)).collect(),
            }),
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let (worst, mean) = difference(&flat, &laid);
    assert!(worst <= 2 && mean < 0.05, "worst {worst}, mean {mean:.3}");
    // Where one sample alone covers, half shows.
    let alone = laid.pixel(32, 60).unwrap()[3];
    assert!(alone.abs_diff(128) <= 1, "{alone}");
}

#[test]
fn a_projected_layer_with_affine_moves_is_a_conflict() {
    let layer = Layer {
        motion: vec![Affine::IDENTITY],
        projection: Some(Projection::new(Homography::IDENTITY)),
        ..card(10.0, 10.0)
    };
    let frame = Frame {
        nodes: vec![Node::Layer(layer)],
        ..Frame::default()
    };
    let e = render_with(&frame, 16, 16, RenderOptions::GOLDEN).unwrap_err();
    assert_eq!(e.kind, Kind::Conflict);
    assert_eq!(
        e.expected.as_deref(),
        Some("motion blur of a projected layer as `Projection::samples`")
    );
}

#[test]
fn opacity_blend_and_matte_apply_after_the_projection() {
    let map = camera(
        300.0,
        Point::new(50.0, 50.0),
        Point::new(60.0, 40.0),
        30.0,
        0.0,
        0.0,
    );
    let half = Layer {
        opacity: 0.5,
        ..card(120.0, 80.0)
    };
    let full = draw(vec![projected(card(120.0, 80.0), map)], 100, 100);
    let faded = draw(vec![projected(half, map)], 100, 100);
    for (a, b) in full.pixels().chunks(4).zip(faded.pixels().chunks(4)) {
        assert!((f64::from(a[3]) / 2.0 - f64::from(b[3])).abs() <= 1.0);
    }
    // A matte in the parent's coordinates: the left half of the canvas.
    let matte = crate::list::Matte {
        source: Box::new(fill(
            Rect::new(0.0, 0.0, 50.0, 100.0).to_path(0.1),
            Color::WHITE,
        )),
        mode: crate::list::MatteMode::Alpha,
    };
    let matted = Layer {
        matte: Some(matte),
        ..card(120.0, 80.0)
    };
    let image = draw(vec![projected(matted, map)], 100, 100);
    assert_eq!(image.pixel(70, 50).unwrap()[3], 0);
    assert_eq!(image.pixel(30, 50), full.pixel(30, 50));
}

#[test]
fn a_projected_filter_applies_where_its_plane_is_seen() {
    let map = camera(
        300.0,
        Point::new(50.0, 50.0),
        Point::new(20.0, 50.0),
        0.0,
        0.0,
        0.0,
    );
    // A white canvas, inverted by a filter whose plane covers its left
    // 40 px around the centre.
    let filter = Layer {
        adjust: true,
        bounds: Some(Rect::new(0.0, 0.0, 40.0, 100.0)),
        effects: vec![Effect::new("invert", &[]).unwrap()],
        projection: Some(Projection::new(map)),
        ..Layer::new(vec![fill(
            Rect::new(0.0, 0.0, 40.0, 100.0).to_path(0.1),
            Color::WHITE,
        )])
    };
    let image = draw(
        vec![
            fill(Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1), Color::WHITE),
            Node::Layer(filter),
        ],
        100,
        100,
    );
    assert_eq!(image.pixel(40, 50).unwrap(), [0, 0, 0, 255]);
    assert_eq!(image.pixel(80, 50).unwrap(), [255, 255, 255, 255]);
}

#[test]
fn a_plane_out_of_view_draws_nothing() {
    // Turned away from the camera, behind it.
    let map = camera(
        300.0,
        Point::new(50.0, 50.0),
        Point::new(60.0, 40.0),
        0.0,
        0.0,
        -400.0,
    );
    let image = draw(vec![projected(card(120.0, 80.0), map)], 100, 100);
    assert!(image.pixels().iter().all(|b| *b == 0));
    // Edge-on: the map is singular.
    let map = camera(
        300.0,
        Point::new(50.0, 50.0),
        Point::new(60.0, 40.0),
        90.0,
        0.0,
        0.0,
    );
    let image = draw(vec![projected(card(120.0, 80.0), map)], 100, 100);
    assert!(image.pixels().chunks(4).all(|p| p[3] < 255));
}

#[test]
fn a_glyph_runs_extent_holds_what_it_draws() {
    let font = FontData::new(Blob::new(Arc::new(FONT)), 0);
    let glyphs = GlyphDraw {
        font,
        size: 48.0,
        glyphs: (0..6)
            .map(|i| crate::list::Glyph {
                id: 9 + i,
                x: 10.0 + i as f32 * 30.0,
                y: 70.0,
            })
            .collect(),
        glyph_transform: Some(Affine::skew(-0.2, 0.0)),
        coords: Vec::new(),
        paint: Paint::Solid(Color::BLACK),
        style: DrawStyle::Fill(FillRule::NonZero),
    };
    let Extent::Box(b) = glyphs_extent(&glyphs) else {
        panic!("a readable font has a box");
    };
    let image = draw(vec![Node::Glyphs(glyphs)], 240, 120);
    let mut drawn = 0;
    for y in 0..120 {
        for x in 0..240 {
            if image.pixel(x, y).unwrap()[3] > 0 {
                drawn += 1;
                let centre = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                assert!(
                    b.inflate(1.0, 1.0).contains(centre),
                    "{centre:?} outside {b:?}"
                );
            }
        }
    }
    assert!(drawn > 500);
}

#[test]
fn extents_grow_by_effects_and_shrink_to_the_clip() {
    let near = |e: Extent, r: Rect| match e {
        Extent::Box(b) => [b.x0 - r.x0, b.y0 - r.y0, b.x1 - r.x1, b.y1 - r.y1]
            .iter()
            .all(|d| d.abs() < 1e-4),
        _ => false,
    };
    let mut layer = card(100.0, 50.0);
    assert_eq!(
        extent(&layer),
        Extent::Box(Rect::new(0.0, 0.0, 100.0, 50.0))
    );
    // A blurriness of 10 is a Gaussian of 3 px, reaching four of them.
    layer.effects =
        vec![Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(10.0))]).unwrap()];
    assert!(near(extent(&layer), Rect::new(-12.0, -12.0, 112.0, 62.0)));
    assert!((read_reach(&layer).unwrap() - 12.0).abs() < 1e-4);
    layer.clip = Some(Clip::Rect(Rect::new(0.0, 0.0, 50.0, 200.0)));
    assert!(near(extent(&layer), Rect::new(0.0, 0.0, 50.0, 62.0)));
    // Tiles grow the box and the clip with it, by how far they reach.
    layer.effects = vec![
        Effect::new(
            "repeat-tile",
            &[
                ("right", ParamValue::Number(30.0)),
                ("up", ParamValue::Number(5.0)),
            ],
        )
        .unwrap(),
    ];
    assert!(near(extent(&layer), Rect::new(0.0, -5.0, 80.0, 50.0)));
    assert_eq!(read_reach(&layer), None);
    layer.effects = vec![Effect::new("particles", &[]).unwrap()];
    layer.clip = None;
    assert_eq!(extent(&layer), Extent::Everywhere);
    assert_eq!(read_reach(&layer), None);
    assert_eq!(nodes_extent(&[]), Extent::Nothing);
}

#[test]
fn a_plane_that_draws_everywhere_ends_short_of_its_horizon() {
    // A floor tipped 80° back, 300 px below the camera's eye: its horizon
    // is in view, so the part the canvas sees has no end.
    let map = camera(300.0, Point::new(96.0, 54.0), Point::ZERO, 0.0, 80.0, 300.0);
    let plan = Plan::new(Extent::Everywhere, Some(0.0), &[map], 192, 108).unwrap();
    assert!(f64::from(plan.width) <= MAX_SIDE && f64::from(plan.height) <= MAX_SIDE);
    // It recedes towards +y, and is cut where 64 by 64 of its units fall
    // within a px of the canvas.
    let far = plan.region.y1;
    assert!(far.is_finite() && far < 1e6, "{:?}", plan.region);
    let [a, b, c, d] = map.jacobian(Point::new(0.0, far)).unwrap();
    let area = (a * d - b * c).abs();
    assert!((area / FAR_AREA - 1.0).abs() < 1e-6, "{area}");
}

#[test]
fn the_buffer_is_capped_in_scale_and_side() {
    let extent = Extent::Box(Rect::new(0.0, 0.0, 100.0, 100.0));
    // Magnified eight times: drawn at four.
    let near = Homography::from(Affine::scale(8.0));
    let plan = Plan::new(extent, Some(0.0), &[near], 1000, 1000).unwrap();
    assert_eq!(plan.local.as_coeffs()[0], MAX_SCALE);
    // Far wider than the cap: its scale falls to fit.
    let wide = Extent::Box(Rect::new(0.0, 0.0, 40_000.0, 10.0));
    let plan = Plan::new(wide, None, &[Homography::IDENTITY], 100, 100).unwrap();
    assert!(f64::from(plan.width) <= MAX_SIDE, "{}", plan.width);
}

/// `card` with one `effect` of `params`, its transform scaled by `own`.
fn card_with(name: &str, params: &[(&str, ParamValue)], own: f64) -> Layer {
    Layer {
        transform: Affine::scale(own),
        effects: vec![Effect::new(name, params).unwrap()],
        ..card(400.0, 200.0)
    }
}

#[rstest]
// A kernel of a px or two, shrunk: drawn at the layer's own size.
#[case::sharpen_shrunk("sharpen", 50.0, 1.0, 0.5, Some(1.0))]
// The layer's own scale is the size it is drawn at.
#[case::sharpen_scaled_up_in_a_shrunk_parent("sharpen", 50.0, 2.0, 0.25, Some(2.0))]
// Not shrunk: drawn where it lands, as before.
#[case::sharpen_not_shrunk("sharpen", 50.0, 1.0, 1.0, None)]
// A blur that still reads many px where it lands.
#[case::wide_blur_shrunk("gaussian-blur", 60.0, 1.0, 0.5, None)]
// A blur of a few px where it lands.
#[case::narrow_blur_shrunk("gaussian-blur", 4.0, 1.0, 0.5, Some(1.0))]
// An effect that changes each pixel alone is exact at any size.
#[case::colour_shrunk("invert", 0.0, 1.0, 0.25, None)]
fn a_shrunk_layer_is_drawn_whole_when_its_effects_read_a_few_px(
    #[case] name: &str,
    #[case] amount: f32,
    #[case] own: f64,
    #[case] shrink: f64,
    #[case] whole: Option<f64>,
) {
    let first = EFFECTS
        .iter()
        .find(|s| s.name == name)
        .unwrap()
        .params
        .first()
        .map(|p| p.name);
    let params: Vec<(&str, ParamValue)> = first
        .filter(|_| amount > 0.0)
        .map(|p| (p, ParamValue::Number(amount)))
        .into_iter()
        .collect();
    let layer = card_with(name, &params, own);
    assert_eq!(whole_scale(&layer, Affine::scale(shrink)), whole);
}

#[test]
fn a_layer_with_moves_for_motion_blur_is_never_drawn_whole() {
    let layer = Layer {
        motion: vec![Affine::IDENTITY, Affine::translate((4.0, 0.0))],
        ..card_with("sharpen", &[], 1.0)
    };
    assert_eq!(whole_scale(&layer, Affine::scale(0.5)), None);
}

// Corner pins ([D89]): a layer whose `corner-pin` lays its box onto four
// corners is drawn as the plane a projected layer is.

/// A `corner-pin` taking `from`'s corners to `to`, top left first and
/// clockwise.
fn pin(from: Rect, to: [Point; 4]) -> Effect {
    let rest = [
        Point::new(from.x0, from.y0),
        Point::new(from.x1, from.y0),
        Point::new(from.x1, from.y1),
        Point::new(from.x0, from.y1),
    ];
    let names = ["top-left", "top-right", "bottom-right", "bottom-left"];
    let params: Vec<(&str, ParamValue)> = (0..4)
        .map(|k| {
            let d = to[k] - rest[k];
            (names[k], ParamValue::Point([d.x as f32, d.y as f32]))
        })
        .collect();
    Effect::new("corner-pin", &params).unwrap()
}

/// A tilted quad inside a 200 by 160 canvas.
fn quad() -> [Point; 4] {
    [
        Point::new(30.0, 20.0),
        Point::new(170.0, 40.0),
        Point::new(150.0, 140.0),
        Point::new(40.0, 120.0),
    ]
}

/// The map a pin of `from` onto [`quad`] lays through, the corners rounded
/// to the effect's `f32` as the pin rounds them.
fn quad_map(from: Rect) -> Homography {
    let effect = pin(from, quad());
    effect.pin_map(from).unwrap()
}

/// Squares on whole px of a `w` by `h` box, so drawing them at any whole
/// multiple of their size and averaging back gives the same px.
fn blocks(w: f64, h: f64) -> Layer {
    let mut children = vec![fill(Rect::new(0.0, 0.0, w, h).to_path(0.1), Color::WHITE)];
    for (i, colour) in [Color::BLACK, Color::rgba(1.0, 0.0, 0.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let x = 10.0 + 40.0 * i as f64;
        children.push(fill(
            Rect::new(x, 10.0, x + 30.0, h - 10.0).to_path(0.1),
            colour,
        ));
    }
    Layer {
        bounds: Some(Rect::new(0.0, 0.0, w, h)),
        ..Layer::new(children)
    }
}

/// The mean difference of `image`'s alpha from that of `layer` laid
/// through `map` by hand: each px the mean of 8 by 8 points mapped back
/// onto the layer drawn flat at 4 times its size.
fn alpha_error(image: &Image, layer: &Layer, map: Homography, w: f64, h: f64) -> f64 {
    let k = 4.0;
    let flat = draw(
        vec![Node::Layer(Layer {
            transform: Affine::scale(k),
            ..layer.clone()
        })],
        (w * k) as u32,
        (h * k) as u32,
    );
    let back = map.inverse().unwrap();
    let (mut sum, mut count) = (0.0, 0);
    for y in 0..image.height() {
        for x in 0..image.width() {
            let mut alpha = 0.0;
            for i in 0..8 {
                for j in 0..8 {
                    let p = Point::new(
                        f64::from(x) + (f64::from(i) + 0.5) / 8.0,
                        f64::from(y) + (f64::from(j) + 0.5) / 8.0,
                    );
                    let Some(u) = back.project(p) else { continue };
                    let (fx, fy) = (u.x * k, u.y * k);
                    if fx >= 0.0 && fy >= 0.0 && fx < w * k && fy < h * k {
                        let px = flat.pixel(fx as u32, fy as u32).unwrap();
                        alpha += f64::from(px[3]) / 255.0 / 64.0;
                    }
                }
            }
            let drawn = f64::from(image.pixel(x, y).unwrap()[3]) / 255.0;
            sum += (drawn - alpha).abs();
            count += 1;
        }
    }
    sum / f64::from(count)
}

#[test]
fn a_pinned_layer_lands_on_its_quad() {
    let from = Rect::new(0.0, 0.0, 120.0, 80.0);
    let pinned = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, quad())],
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let error = alpha_error(&pinned, &card(120.0, 80.0), quad_map(from), 120.0, 80.0);
    assert!(error < 0.002, "mean alpha error {error:.4}");
    // Its corners land where they were asked to.
    assert!(pinned.pixel(32, 22).unwrap()[3] > 0);
    assert_eq!(pinned.pixel(25, 15).unwrap()[3], 0);
}

#[test]
fn a_pin_that_moves_nothing_draws_as_the_flat_layer() {
    let flat = draw(vec![Node::Layer(blocks(120.0, 80.0))], 200, 160);
    let pinned = draw(
        vec![Node::Layer(Layer {
            effects: vec![Effect::new("corner-pin", &[]).unwrap()],
            ..blocks(120.0, 80.0)
        })],
        200,
        160,
    );
    let (worst, mean) = difference(&flat, &pinned);
    assert!(worst <= 1 && mean < 0.01, "worst {worst}, mean {mean:.4}");
}

#[test]
fn a_pin_lays_through_the_layers_own_transform_too() {
    let from = Rect::new(0.0, 0.0, 120.0, 80.0);
    let at = Affine::translate((-20.0, 10.0)) * Affine::rotate(0.1);
    let to = quad().map(|p| at.inverse() * p);
    let moved = draw(
        vec![Node::Layer(Layer {
            transform: at,
            effects: vec![pin(from, to)],
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let still = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, quad())],
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let (worst, mean) = difference(&moved, &still);
    assert!(worst <= 2 && mean < 0.02, "worst {worst}, mean {mean:.4}");
}

#[test]
fn effects_before_a_pin_run_on_its_plane_and_after_it_on_what_it_laid() {
    let from = Rect::new(0.0, 0.0, 120.0, 80.0);
    let blur = || Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(6.0))]).unwrap();
    let invert = || Effect::new("invert", &[]).unwrap();
    // Before: the blur spreads in the layer's own px, as the same blur on
    // a layer inside the pinned one.
    let before = draw(
        vec![Node::Layer(Layer {
            effects: vec![blur(), pin(from, quad())],
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    // The outer layer has no box of its own, so its extent holds the
    // blur's spill; the pin is given the box it rests on.
    let inside = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, quad()).with_rest(from)],
            ..Layer::new(vec![Node::Layer(Layer {
                effects: vec![blur()],
                ..card(120.0, 80.0)
            })])
        })],
        200,
        160,
    );
    let (worst, mean) = difference(&before, &inside);
    assert!(worst <= 1 && mean < 0.01, "worst {worst}, mean {mean:.4}");
    // After: the inversion runs on the pinned quad, as on a layer around it.
    let after = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, quad()), invert()],
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let around = draw(
        vec![Node::Layer(Layer {
            effects: vec![invert()],
            ..Layer::new(vec![Node::Layer(Layer {
                effects: vec![pin(from, quad())],
                ..card(120.0, 80.0)
            })])
        })],
        200,
        160,
    );
    let (worst, mean) = difference(&after, &around);
    assert!(worst <= 1 && mean < 0.01, "worst {worst}, mean {mean:.4}");
}

#[test]
fn a_pin_onto_no_quad_draws_nothing() {
    let from = Rect::new(0.0, 0.0, 120.0, 80.0);
    let mut crossed = quad();
    crossed.swap(2, 3);
    let image = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, crossed)],
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    assert!(image.pixels().chunks(4).all(|p| p[3] == 0));
}

#[test]
fn a_box_mostly_off_the_canvas_is_pinned_whole() {
    // A box far larger than the canvas, at its own size mostly off it,
    // pinned into view: all of it shows, as a plane draws what it sees.
    let from = Rect::new(0.0, 0.0, 1200.0, 800.0);
    let pinned = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, quad())],
            ..card(1200.0, 800.0)
        })],
        200,
        160,
    );
    let error = alpha_error(&pinned, &card(1200.0, 800.0), quad_map(from), 1200.0, 800.0);
    assert!(error < 0.002, "mean alpha error {error:.4}");
    // The red bar, 60 to 70 % across the box, is in view.
    let bar = quad_map(from).project(Point::new(780.0, 400.0)).unwrap();
    let px = pinned.pixel(bar.x as u32, bar.y as u32).unwrap();
    assert!(px[0] > 200 && px[1] < 60, "{px:?}");
}

#[test]
fn a_pin_on_a_filter_lays_what_is_below_through_its_map() {
    let from = Rect::new(0.0, 0.0, 200.0, 160.0);
    let below = || Layer {
        isolate: true,
        bounds: Some(from),
        ..Layer::new(vec![
            fill(from.to_path(0.1), Color::WHITE),
            fill(Circle::new((70.0, 80.0), 40.0).to_path(0.1), Color::BLACK),
        ])
    };
    let filter = Layer {
        adjust: true,
        effects: vec![pin(from, quad())],
        ..Layer::new(Vec::new())
    };
    let warped = draw(vec![Node::Layer(below()), Node::Layer(filter)], 200, 160);
    let pinned = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, quad())],
            ..below()
        })],
        200,
        160,
    );
    // What is below is laid from its own grid, the layer from a plane drawn
    // finer, so only their edges differ.
    let (_, mean) = difference(&warped, &pinned);
    assert!(mean < 1.0, "mean {mean:.4}");
    let map = quad_map(from);
    for p in [Point::new(70.0, 80.0), Point::new(150.0, 80.0)] {
        let at = map.project(p).unwrap();
        let (x, y) = (at.x as u32, at.y as u32);
        assert_eq!(warped.pixel(x, y), pinned.pixel(x, y), "at {p:?}");
    }
    assert_eq!(warped.pixel(5, 5).unwrap()[3], 0);
}

#[test]
fn a_pins_moments_blur_as_its_moments_drawn_alone_and_averaged() {
    let from = Rect::new(0.0, 0.0, 120.0, 80.0);
    let shifted = |dx: f64| quad().map(|p| p + vello_cpu::kurbo::Vec2::new(dx, 0.0));
    let moments = [shifted(-8.0), shifted(8.0)];
    let moves = [
        Affine::translate((0.0, -4.0)),
        Affine::translate((0.0, 4.0)),
    ];
    let effect = pin(from, quad())
        .with_moments(moments.iter().map(|q| pin(from, *q)))
        .unwrap();
    let blurred = draw(
        vec![Node::Layer(Layer {
            effects: vec![effect],
            motion: moves.to_vec(),
            ..card(120.0, 80.0)
        })],
        200,
        160,
    );
    let alone = moves
        .iter()
        .zip(&moments)
        .map(|(m, q)| {
            Node::Layer(Layer {
                transform: *m,
                opacity: 0.5,
                blend: crate::blend::BlendMode::Add,
                effects: vec![pin(from, *q)],
                ..card(120.0, 80.0)
            })
        })
        .collect();
    let averaged = draw(
        vec![Node::Layer(Layer {
            isolate: true,
            ..Layer::new(alone)
        })],
        200,
        160,
    );
    let (worst, mean) = difference(&blurred, &averaged);
    assert!(worst <= 3 && mean < 0.05, "worst {worst}, mean {mean:.4}");
    // Where one moment alone covers, half shows.
    let left = shifted(-8.0)[3] + vello_cpu::kurbo::Vec2::new(1.5, -4.0 - 3.0);
    let alpha = blurred.pixel(left.x as u32, left.y as u32).unwrap()[3];
    assert!(alpha.abs_diff(128) <= 8, "{alpha}");
}

#[test]
fn a_pin_with_moments_other_than_its_layers_moves_is_a_conflict() {
    let from = Rect::new(0.0, 0.0, 120.0, 80.0);
    let effect = pin(from, quad()).with_moments([pin(from, quad())]).unwrap();
    let frame = Frame {
        nodes: vec![Node::Layer(Layer {
            effects: vec![effect],
            motion: vec![Affine::IDENTITY, Affine::IDENTITY],
            ..card(120.0, 80.0)
        })],
        ..Frame::default()
    };
    let e = render_with(&frame, 32, 32, RenderOptions::GOLDEN).unwrap_err();
    assert_eq!(e.kind, Kind::Conflict);
}

#[test]
fn a_moment_of_another_effect_is_refused() {
    let e = Effect::new("corner-pin", &[])
        .unwrap()
        .with_moments([Effect::new("invert", &[]).unwrap()])
        .unwrap_err();
    assert_eq!(e.kind, Kind::Conflict);
}

#[test]
fn a_pinned_layers_extent_is_its_quads_box() {
    let from = Rect::new(0.0, 0.0, 120.0, 80.0);
    let layer = Layer {
        effects: vec![pin(from, quad())],
        ..card(120.0, 80.0)
    };
    let Extent::Box(b) = extent(&layer) else {
        panic!("a box")
    };
    let want = Rect::new(30.0, 20.0, 170.0, 140.0);
    for (got, want) in [
        (b.x0, want.x0),
        (b.y0, want.y0),
        (b.x1, want.x1),
        (b.y1, want.y1),
    ] {
        assert!((got - want).abs() < 1e-3, "{b:?}");
    }
}

/// A UI card: a white `w` by `h` panel with lines of black glyphs `size`
/// px tall, as an app's screen holds text.
fn ui(w: f64, h: f64, size: f32) -> Layer {
    let font = FontData::new(Blob::new(Arc::new(FONT)), 0);
    let mut children = vec![fill(Rect::new(0.0, 0.0, w, h).to_path(0.1), Color::WHITE)];
    let advance = size * 0.62;
    let lines = ((h as f32 - size) / (size * 1.5)) as usize;
    for line in 0..lines {
        let count = ((w as f32 - 2.0 * size) / advance) as u16;
        children.push(Node::Glyphs(GlyphDraw {
            font: font.clone(),
            size,
            glyphs: (0..count)
                .map(|i| crate::list::Glyph {
                    id: 9 + u32::from((i * 7 + line as u16 * 3) % 40),
                    x: size + f32::from(i) * advance,
                    y: size * 1.4 + line as f32 * size * 1.5,
                })
                .collect(),
            glyph_transform: None,
            coords: Vec::new(),
            paint: Paint::Solid(Color::BLACK),
            style: DrawStyle::Fill(FillRule::NonZero),
        }));
    }
    Layer {
        bounds: Some(Rect::new(0.0, 0.0, w, h)),
        ..Layer::new(children)
    }
}

/// The luma of each px of `image` inside `inside` (canvas px), 0 to 1, as
/// it shows over black.
fn lumas(image: &Image, inside: &dyn Fn(Point) -> bool) -> Vec<Option<f64>> {
    let (w, h) = (image.width(), image.height());
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let centre = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            inside(centre).then(|| {
                let p = image.pixel(x, y).unwrap();
                let a = f64::from(p[3]) / 255.0;
                (0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]))
                    / 255.0
                    * a
            })
        })
        .collect()
}

/// The mean absolute difference of `a` from `reference` where both are
/// measured, and the ratio of their gradient energies there (1 when as
/// sharp as the reference, below when blurred, above when aliased).
fn against(a: &[Option<f64>], reference: &[Option<f64>], width: usize) -> (f64, f64) {
    let (mut error, mut count) = (0.0, 0);
    let (mut ga, mut gr) = (0.0, 0.0);
    for i in 0..a.len() {
        let (Some(x), Some(r)) = (a[i], reference[i]) else {
            continue;
        };
        error += (x - r).abs();
        count += 1;
        for j in [i + 1, i + width] {
            if let (Some(Some(xn)), Some(Some(rn))) = (a.get(j), reference.get(j)) {
                ga += (xn - x).powi(2);
                gr += (rn - r).powi(2);
            }
        }
    }
    (error / f64::from(count), (ga / gr).sqrt())
}

/// `image`'s straight 8-bit px as premultiplied floats.
fn premultiplied(image: &Image) -> Pixels {
    let data = image
        .pixels()
        .chunks(4)
        .map(|p| {
            let a = f32::from(p[3]) / 255.0;
            [
                f32::from(p[0]) / 255.0 * a,
                f32::from(p[1]) / 255.0 * a,
                f32::from(p[2]) / 255.0 * a,
                a,
            ]
        })
        .collect();
    Pixels::from_data(image.width() as u16, image.height() as u16, data)
}

/// Pinned text against a supersampled reference, for a UI `w` by `h`
/// with glyphs `size` px tall pinned onto `to` on a `cw` by `ch` canvas:
/// the pin, and the same pin made by moving pixels (the layer drawn at its
/// own size, each px of the canvas read bilinearly through the map, as
/// the distortions read). Each as `(mean error, gradient ratio)`.
fn pinned_text(
    (w, h, size): (f64, f64, f32),
    to: [Point; 4],
    (cw, ch): (u32, u32),
) -> ((f64, f64), (f64, f64)) {
    let from = Rect::new(0.0, 0.0, w, h);
    let map = pin(from, to).pin_map(from).unwrap();
    let back = map.inverse().unwrap();
    let pinned = draw(
        vec![Node::Layer(Layer {
            effects: vec![pin(from, to)],
            ..ui(w, h, size)
        })],
        cw,
        ch,
    );
    // The reference: the UI drawn flat at 4 times its size, each px of the
    // canvas the mean of 8 by 8 points mapped back onto it.
    let k = 4.0;
    let fine = draw(
        vec![Node::Layer(Layer {
            transform: Affine::scale(k),
            ..ui(w, h, size)
        })],
        (w * k) as u32,
        (h * k) as u32,
    );
    let fine = premultiplied(&fine);
    let flat = draw(vec![Node::Layer(ui(w, h, size))], w as u32, h as u32);
    let flat = premultiplied(&flat);
    let mut reference = vec![0u8; (cw * ch * 4) as usize];
    let mut moved = vec![0u8; (cw * ch * 4) as usize];
    for y in 0..ch {
        for x in 0..cw {
            let mut sum = [0.0f32; 4];
            for i in 0..8 {
                for j in 0..8 {
                    let p = Point::new(
                        f64::from(x) + (f64::from(i) + 0.5) / 8.0,
                        f64::from(y) + (f64::from(j) + 0.5) / 8.0,
                    );
                    let Some(u) = back.project(p) else { continue };
                    let s = fine.sample(u.x * k, u.y * k);
                    for c in 0..4 {
                        sum[c] += s[c] / 64.0;
                    }
                }
            }
            let at = (((y * cw) + x) * 4) as usize;
            let straight = |px: [f32; 4]| -> [u8; 4] {
                let a = px[3];
                let un = |c: f32| match a > 0.0 {
                    true => ((c / a).clamp(0.0, 1.0) * 255.0).round() as u8,
                    false => 0,
                };
                [
                    un(px[0]),
                    un(px[1]),
                    un(px[2]),
                    (a.clamp(0.0, 1.0) * 255.0).round() as u8,
                ]
            };
            reference[at..at + 4].copy_from_slice(&straight(sum));
            let centre = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            if let Some(u) = back.project(centre) {
                moved[at..at + 4].copy_from_slice(&straight(flat.sample(u.x, u.y)));
            }
        }
    }
    let reference = Image::new(cw, ch, reference).unwrap();
    let moved = Image::new(cw, ch, moved).unwrap();
    // Measured inside the quad, a px in from its edges.
    let inside = |p: Point| {
        back.project(p)
            .is_some_and(|u| u.x > 1.0 && u.y > 1.0 && u.x < w - 1.0 && u.y < h - 1.0)
    };
    let (r, a, b) = (
        lumas(&reference, &inside),
        lumas(&pinned, &inside),
        lumas(&moved, &inside),
    );
    (against(&a, &r, cw as usize), against(&b, &r, cw as usize))
}

#[test]
fn pinned_text_shrunk_into_perspective_stays_sharp_without_aliasing() {
    // A phone UI 360 by 720 with 14 px text, pinned onto a screen about a
    // quarter its size, leaning away.
    let to = [
        Point::new(70.0, 40.0),
        Point::new(150.0, 52.0),
        Point::new(160.0, 230.0),
        Point::new(62.0, 240.0),
    ];
    let (pin, moved) = pinned_text((360.0, 720.0, 14.0), to, (220, 280));
    eprintln!(
        "shrunk: pin error {:.4} sharpness {:.3}; moved pixels error {:.4} sharpness {:.3}",
        pin.0, pin.1, moved.0, moved.1
    );
    assert!(pin.0 < 0.6 * moved.0, "pin {pin:?}, moved {moved:?}");
    assert!((0.85..1.15).contains(&pin.1), "pin {pin:?}");
}

#[test]
fn pinned_text_enlarged_into_perspective_stays_sharp() {
    // A small UI with 7 px text pinned onto a quad about three times its
    // size.
    let to = [
        Point::new(20.0, 30.0),
        Point::new(330.0, 10.0),
        Point::new(340.0, 300.0),
        Point::new(10.0, 260.0),
    ];
    let (pin, moved) = pinned_text((110.0, 100.0, 7.0), to, (360, 320));
    eprintln!(
        "enlarged: pin error {:.4} sharpness {:.3}; moved pixels error {:.4} sharpness {:.3}",
        pin.0, pin.1, moved.0, moved.1
    );
    assert!(pin.0 < 0.6 * moved.0, "pin {pin:?}, moved {moved:?}");
    assert!(pin.1 > moved.1, "pin {pin:?}, moved {moved:?}");
    assert!((0.85..1.15).contains(&pin.1), "pin {pin:?}");
}
