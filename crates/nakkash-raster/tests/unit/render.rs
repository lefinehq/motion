//! Unit tests for `src/render.rs`: sizes, settings, and the colours that
//! compositing gives, checked as numbers.

use super::*;
use crate::blend::BlendMode;
use crate::effect::{Effect, ParamValue};
use crate::list::{Clip, Shared};
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
fn an_inverted_fill_fills_the_rest_of_the_box_and_clears_the_content() {
    let inverted = Effect::new(
        "fill",
        &[
            ("color", ParamValue::Color(Color::WHITE)),
            ("inverted", ParamValue::Choice(1)),
        ],
    )
    .unwrap();
    // Exposure at its rest changes nothing but reads the pixels, so the
    // fill runs on them rather than painted over the content.
    let reads = Effect::new("exposure", &[]).unwrap();
    assert!(reads.reads_pixels() && !inverted.reads_pixels());
    // A red strip two pixels wide at the left of a box six pixels wide.
    let strip = Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, 2.0, 8.0).to_path(0.1),
        paint: Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        style: DrawStyle::Fill(FillRule::NonZero),
    });
    for effects in [vec![inverted.clone()], vec![reads, inverted]] {
        let frame = Frame {
            nodes: vec![Node::Layer(Layer {
                effects,
                bounds: Some(Rect::new(0.0, 0.0, 6.0, 8.0)),
                ..Layer::new(vec![strip.clone()])
            })],
            ..Frame::default()
        };
        let image = render_with(&frame, 8, 8, RenderOptions::GOLDEN).unwrap();
        // The strip is cleared, the rest of the box filled, and nothing
        // shows beyond the box.
        let [strip, rest, beyond] = [1, 4, 7].map(|x| image.pixel(x, 4).unwrap());
        assert!(near(strip, [0, 0, 0, 0]), "{strip:?}");
        assert!(near(rest, [255, 255, 255, 255]), "{rest:?}");
        assert!(near(beyond, [0, 0, 0, 0]), "{beyond:?}");
    }
}

/// The transform effect with `position`, `anchor`, `scale` and `opacity`.
fn transform(position: [f32; 2], anchor: [f32; 2], scale: [f32; 2], opacity: f32) -> Effect {
    Effect::new(
        "transform",
        &[
            ("position", ParamValue::Point(position)),
            ("anchor", ParamValue::Point(anchor)),
            ("scale", ParamValue::Point(scale)),
            ("opacity", ParamValue::Number(opacity)),
        ],
    )
    .unwrap()
}

#[test]
fn the_transform_effect_moves_and_fades_the_content_wherever_it_stands() {
    // A 2-pixel square in the top left corner of an 8-pixel frame, moved by
    // its anchor to the centre and scaled up three times about it.
    let square = Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, 2.0, 2.0).to_path(0.1),
        paint: Paint::Solid(Color::BLACK),
        style: DrawStyle::Fill(FillRule::NonZero),
    });
    let frame = |effects| Frame {
        nodes: vec![Node::Layer(Layer {
            effects,
            ..Layer::new(vec![square.clone()])
        })],
        ..Frame::default()
    };
    let moved = transform([4.0, 4.0], [1.0, 1.0], [3.0, 3.0], 0.5);
    let image = render_with(&frame(vec![moved.clone()]), 8, 8, RenderOptions::GOLDEN).unwrap();
    // Now 6 pixels wide about the centre, at half opacity; the corner is empty.
    assert!(
        near(image.pixel(2, 2).unwrap(), [0, 0, 0, 128]),
        "{:?}",
        image.pixel(2, 2)
    );
    assert!(
        near(image.pixel(5, 5).unwrap(), [0, 0, 0, 128]),
        "{:?}",
        image.pixel(5, 5)
    );
    assert_eq!(image.pixel(0, 0).unwrap(), [0, 0, 0, 0]);
    // A fill after it recolours what it moved, as a fill before it does.
    let white = Effect::new("fill", &[("color", ParamValue::Color(Color::WHITE))]).unwrap();
    for effects in [vec![moved.clone(), white.clone()], vec![white, moved]] {
        let image = render_with(&frame(effects), 8, 8, RenderOptions::GOLDEN).unwrap();
        assert!(
            near(image.pixel(5, 5).unwrap(), [255, 255, 255, 128]),
            "{:?}",
            image.pixel(5, 5)
        );
    }
}

#[test]
fn transform_effects_apply_in_order_each_moving_what_the_last_moved() {
    let effects = [
        transform([2.0, 0.0], [0.0, 0.0], [1.0, 1.0], 1.0),
        transform([0.0, 0.0], [0.0, 0.0], [2.0, 2.0], 1.0),
    ];
    let movement = |list: &[Effect]| {
        list.iter()
            .filter_map(Effect::movement)
            .fold(Affine::IDENTITY, |done, next| next.transform * done)
    };
    // Moved 2 right, then doubled about the origin: 1 goes to 6.
    assert_eq!(
        movement(&effects) * vello_cpu::kurbo::Point::new(1.0, 0.0),
        (6.0, 0.0).into()
    );
}

/// A filter layer applying `effects` at `opacity`.
fn filter(effects: Vec<Effect>, opacity: f32) -> Node {
    Node::Layer(Layer {
        effects,
        opacity,
        adjust: true,
        ..Layer::new(Vec::new())
    })
}

#[test]
fn a_filter_layer_changes_what_is_below_it_and_not_what_is_above() {
    let white = Effect::new("fill", &[("color", ParamValue::Color(Color::WHITE))]).unwrap();
    let red = cover(8.0, Color::rgba(1.0, 0.0, 0.0, 1.0));
    let half_blue = Node::Path(PathDraw {
        path: Rect::new(4.0, 0.0, 8.0, 8.0).to_path(0.1),
        paint: Paint::Solid(Color::rgba(0.0, 0.0, 1.0, 1.0)),
        style: DrawStyle::Fill(FillRule::NonZero),
    });
    let frame = |opacity| Frame {
        background: Some(Color::BLACK),
        nodes: vec![
            red.clone(),
            filter(vec![white.clone()], opacity),
            half_blue.clone(),
        ],
        ..Frame::default()
    };
    let image = render_with(&frame(1.0), 8, 8, RenderOptions::GOLDEN).unwrap();
    // The red below turned white; the blue above stays.
    assert!(
        near(image.pixel(1, 4).unwrap(), [255, 255, 255, 255]),
        "{:?}",
        image.pixel(1, 4)
    );
    assert!(
        near(image.pixel(6, 4).unwrap(), [0, 0, 255, 255]),
        "{:?}",
        image.pixel(6, 4)
    );
    // At half opacity, half of the change.
    let image = render_with(&frame(0.5), 8, 8, RenderOptions::GOLDEN).unwrap();
    assert!(
        near(image.pixel(1, 4).unwrap(), [255, 128, 128, 255]),
        "{:?}",
        image.pixel(1, 4)
    );
}

#[test]
fn an_adjusting_layer_applies_only_where_its_children_draw() {
    let white = Effect::new("fill", &[("color", ParamValue::Color(Color::WHITE))]).unwrap();
    let left = Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, 4.0, 8.0).to_path(0.1),
        paint: Paint::Solid(Color::rgba(0.0, 1.0, 0.0, 0.5)),
        style: DrawStyle::Fill(FillRule::NonZero),
    });
    let adjusting = Node::Layer(Layer {
        effects: vec![white],
        adjust: true,
        ..Layer::new(vec![left])
    });
    let frame = Frame {
        nodes: vec![cover(8.0, Color::BLACK), adjusting],
        ..Frame::default()
    };
    let image = render_with(&frame, 8, 8, RenderOptions::GOLDEN).unwrap();
    // Half the change where it draws at half alpha; its own colour never shows.
    assert!(
        near(image.pixel(1, 4).unwrap(), [128, 128, 128, 255]),
        "{:?}",
        image.pixel(1, 4)
    );
    assert_eq!(image.pixel(6, 4).unwrap(), [0, 0, 0, 255]);
}

#[test]
fn a_filter_layer_moves_what_is_below_and_its_matte_limits_it() {
    let square = Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, 2.0, 2.0).to_path(0.1),
        paint: Paint::Solid(Color::BLACK),
        style: DrawStyle::Fill(FillRule::NonZero),
    });
    let moved = transform([4.0, 4.0], [0.0, 0.0], [1.0, 1.0], 1.0);
    let image = render_with(
        &Frame {
            nodes: vec![square.clone(), filter(vec![moved.clone()], 1.0)],
            ..Frame::default()
        },
        8,
        8,
        RenderOptions::GOLDEN,
    )
    .unwrap();
    assert_eq!(image.pixel(0, 0).unwrap(), [0, 0, 0, 0]);
    assert_eq!(image.pixel(5, 5).unwrap(), [0, 0, 0, 255]);
    // A matte over the left half: there the square moves away, elsewhere
    // nothing changes, so the moved copy on the right does not appear.
    let left = Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, 4.0, 8.0).to_path(0.1),
        paint: Paint::Solid(Color::WHITE),
        style: DrawStyle::Fill(FillRule::NonZero),
    });
    let Node::Layer(matted) = filter(vec![moved], 1.0) else {
        unreachable!()
    };
    let matted = Node::Layer(Layer {
        matte: Some(Matte {
            source: Box::new(left),
            mode: MatteMode::Alpha,
        }),
        ..matted
    });
    let image = render_with(
        &Frame {
            nodes: vec![square, matted],
            ..Frame::default()
        },
        8,
        8,
        RenderOptions::GOLDEN,
    )
    .unwrap();
    assert_eq!(image.pixel(0, 0).unwrap(), [0, 0, 0, 0]);
    assert_eq!(image.pixel(5, 5).unwrap(), [0, 0, 0, 0]);
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

#[test]
fn an_effect_that_grows_the_box_grows_the_clip_with_it() {
    // A black 8 by 8 square clipped to its left half, tiled 4 px right.
    let tile = |right: f32| Layer {
        clip: Some(Clip::Rect(Rect::new(0.0, 0.0, 4.0, 8.0))),
        bounds: Some(Rect::new(0.0, 0.0, 4.0, 8.0)),
        effects: vec![
            Effect::new(
                "repeat-tile",
                &[
                    ("right", ParamValue::Number(right)),
                    ("tiling", ParamValue::Choice(1)),
                ],
            )
            .unwrap(),
        ],
        ..Layer::new(vec![
            cover(8.0, Color::BLACK),
            Node::Path(PathDraw {
                path: Rect::new(0.0, 0.0, 1.0, 8.0).to_path(0.1),
                paint: Paint::Solid(Color::WHITE),
                style: DrawStyle::Fill(FillRule::NonZero),
            }),
        ])
    };
    let drawn = |right| {
        render_with(
            &Frame {
                nodes: vec![Node::Layer(tile(right))],
                ..Frame::default()
            },
            8,
            8,
            RenderOptions::GOLDEN,
        )
        .unwrap()
    };
    // The content is cut to the clip before the tiles: the unfolded tile
    // beside it shows its white column at its far edge, not the black
    // beyond the clip.
    let image = drawn(4.0);
    assert_eq!(image.pixel(0, 4), Some([255, 255, 255, 255]));
    assert_eq!(image.pixel(3, 4), Some([0, 0, 0, 255]));
    assert_eq!(image.pixel(4, 4), Some([0, 0, 0, 255]));
    assert_eq!(image.pixel(7, 4), Some([255, 255, 255, 255]));
    // Tiles reaching 2 px show 2 px past the clip.
    let image = drawn(2.0);
    assert_eq!(image.pixel(5, 4), Some([0, 0, 0, 255]));
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
    let image = render_with(
        &frame,
        8,
        8,
        RenderOptions {
            quality,
            simd,
            dither: false,
        },
    )
    .unwrap();
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

/// A ramp from a dim blue at the left of a `width`-px strip to black at
/// its right, as an effect: what a faint light falling off looks like.
fn dim_ramp(width: f32) -> Effect {
    Effect::new(
        "ramp",
        &[
            ("start", ParamValue::Point([0.0, 0.0])),
            (
                "start-color",
                ParamValue::Color(Color::rgba(5.0 / 255.0, 10.0 / 255.0, 34.0 / 255.0, 1.0)),
            ),
            ("end", ParamValue::Point([width, 0.0])),
            ("end-color", ParamValue::Color(Color::BLACK)),
        ],
    )
    .unwrap()
}

/// Four stops brighter, in linear light.
fn four_stops() -> Effect {
    Effect::new(
        "exposure",
        &[
            ("exposure", ParamValue::Number(4.0)),
            ("space", ParamValue::Choice(0)),
        ],
    )
    .unwrap()
}

/// How many different colours the middle row of `frame` holds, drawn
/// `width` by 4 px with `quality`.
fn distinct_colours(frame: &Frame, width: u32, quality: Quality) -> usize {
    let options = RenderOptions {
        quality,
        simd: Simd::Portable,
        dither: false,
    };
    let image = render_with(frame, width, 4, options).unwrap();
    let row: std::collections::BTreeSet<[u8; 4]> =
        (0..width).map(|x| image.pixel(x, 2).unwrap()).collect();
    row.len()
}

/// A dim ramp graded four stops: on the layer that draws it, through a
/// filter layer over it, through a composition holding it, and through a
/// plane holding it. Each grade reads the ramp's float pixels, so each
/// keeps as many colours as the grade on the layer itself; read at 8 bits,
/// a dim ramp has a few dozen levels, which the grade spreads into bands.
#[rstest]
#[case::through_a_filter_layer("filter")]
#[case::through_a_composition("composition")]
#[case::through_a_plane("plane")]
fn precise_grades_keep_the_floats_effects_made(#[case] path: &str) {
    const WIDTH: u32 = 940;
    let strip = Rect::new(0.0, 0.0, f64::from(WIDTH), 4.0);
    let ramp = |effects: Vec<Effect>| {
        Node::Layer(Layer {
            effects,
            bounds: Some(strip),
            ..Layer::new(vec![cover(f64::from(WIDTH), Color::BLACK)])
        })
    };
    let frame = |nodes: Vec<Node>| Frame {
        background: Some(Color::BLACK),
        nodes,
        ..Frame::default()
    };
    let on_the_layer = frame(vec![ramp(vec![dim_ramp(WIDTH as f32), four_stops()])]);
    let holding = |extra: &dyn Fn(Layer) -> Layer| {
        Node::Layer(extra(Layer {
            effects: vec![four_stops()],
            isolate: true,
            ..Layer::new(vec![ramp(vec![dim_ramp(WIDTH as f32)])])
        }))
    };
    let graded = match path {
        "filter" => frame(vec![
            ramp(vec![dim_ramp(WIDTH as f32)]),
            filter(vec![four_stops()], 1.0),
        ]),
        "composition" => frame(vec![holding(&|layer| layer)]),
        _ => frame(vec![holding(&|layer| Layer {
            projection: Some(Projection::new(Homography::IDENTITY)),
            bounds: Some(strip),
            ..layer
        })]),
    };
    let wanted = distinct_colours(&on_the_layer, WIDTH, Quality::Precise);
    let precise = distinct_colours(&graded, WIDTH, Quality::Precise);
    let eight_bit = distinct_colours(&graded, WIDTH, Quality::Fast);
    // A step or two of rounding apart, and far more than at 8 bits.
    assert!(
        precise + 2 >= wanted && precise > eight_bit * 2,
        "{path}: {precise} colours, {wanted} on the layer, {eight_bit} at 8 bits"
    );
}

/// Every blend mode lays a float layer as the context lays a drawn one:
/// within a step of rounding, over a backdrop of several colours.
#[rstest]
fn the_float_canvas_blends_as_the_context_does(
    #[values(
        "normal",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "color-dodge",
        "color-burn",
        "hard-light",
        "soft-light",
        "difference",
        "exclusion",
        "hue",
        "saturation",
        "color",
        "luminosity",
        "add"
    )]
    mode: &str,
) {
    let blend = BlendMode::named(mode).unwrap();
    // Four backdrop colours in columns, then a half-opaque layer over all.
    let column = |x: f64, color: Color| {
        Node::Path(PathDraw {
            path: Rect::new(x, 0.0, x + 2.0, 8.0).to_path(0.1),
            paint: Paint::Solid(color),
            style: DrawStyle::Fill(FillRule::NonZero),
        })
    };
    let backdrop = [
        column(0.0, Color::rgba(0.9, 0.2, 0.1, 1.0)),
        column(2.0, Color::rgba(0.1, 0.6, 0.3, 1.0)),
        column(4.0, Color::rgba(0.3, 0.3, 0.8, 0.6)),
        column(6.0, Color::rgba(0.95, 0.9, 0.85, 1.0)),
    ];
    let layer = |effects: Vec<Effect>| {
        Node::Layer(Layer {
            blend,
            opacity: 0.8,
            effects,
            ..Layer::new(vec![cover(8.0, Color::rgba(0.4, 0.7, 0.55, 0.75))])
        })
    };
    let drawn = |top: Node| {
        let frame = Frame {
            background: Some(Color::rgba(0.5, 0.5, 0.5, 1.0)),
            nodes: backdrop.iter().cloned().chain([top]).collect(),
            ..Frame::default()
        };
        render_with(&frame, 8, 8, RenderOptions::GOLDEN).unwrap()
    };
    // Exposure at rest reads the pixels, so the layer is laid in floats.
    let floated = drawn(layer(vec![Effect::new("exposure", &[]).unwrap()]));
    let drawn_by_the_context = drawn(layer(Vec::new()));
    let worst = floated
        .pixels()
        .iter()
        .zip(drawn_by_the_context.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max();
    assert!(worst <= Some(1), "{mode}: {worst:?}");
}

/// A layer read by several effects is drawn once a frame and shared: the
/// frame is the same, pixel for pixel, as when each effect reads a copy of
/// its own.
#[test]
fn a_layer_read_by_several_effects_draws_the_same_shared_or_not() {
    let map = Node::Layer(Layer::new(vec![Node::Path(PathDraw {
        path: Rect::new(0.0, 0.0, 16.0, 8.0).to_path(0.1),
        paint: Paint::Solid(Color::rgba(0.9, 0.2, 0.6, 1.0)),
        style: DrawStyle::Fill(FillRule::NonZero),
    })]));
    let displaced = |map: Arc<Node>, color: Color| {
        Node::Layer(Layer {
            effects: vec![
                Effect::new(
                    "displacement-map",
                    &[("horizontal", ParamValue::Number(6.0))],
                )
                .unwrap()
                .with_input(map),
            ],
            ..Layer::new(vec![cover(16.0, color)])
        })
    };
    let frame = |share: bool| {
        let shared = Arc::new(map.clone());
        let read = || match share {
            true => Arc::clone(&shared),
            false => Arc::new(map.clone()),
        };
        Frame {
            nodes: vec![
                displaced(read(), Color::rgba(1.0, 0.0, 0.0, 1.0)),
                displaced(read(), Color::rgba(0.0, 0.0, 1.0, 0.5)),
                displaced(read(), Color::rgba(0.0, 1.0, 0.0, 0.5)),
            ],
            ..Frame::default()
        }
    };
    let drawn = |share| render_with(&frame(share), 16, 16, RenderOptions::default()).unwrap();
    assert_eq!(drawn(true), drawn(false));
}

/// Layers marked as sharing their children draw them once a frame: the
/// frame is the same, pixel for pixel, as without the mark.
#[test]
fn layers_sharing_their_children_draw_the_same_marked_or_not() {
    let children = || {
        vec![Node::Path(PathDraw {
            path: Rect::new(2.0, 2.0, 12.0, 9.0).to_path(0.1),
            paint: Paint::Solid(Color::rgba(0.9, 0.3, 0.1, 0.8)),
            style: DrawStyle::Fill(FillRule::NonZero),
        })]
    };
    let frame = |marked: bool| {
        let shared = Shared::new();
        let layer = |dx: f64, blur: f32| {
            Node::Layer(Layer {
                transform: Affine::translate((dx, 0.0)),
                effects: vec![
                    Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(blur))])
                        .unwrap(),
                ],
                isolate: true,
                shared: marked.then(|| shared.clone()),
                ..Layer::new(children())
            })
        };
        Frame {
            nodes: vec![layer(0.0, 2.0), layer(0.0, 4.0), layer(1.5, 2.0)],
            ..Frame::default()
        }
    };
    let drawn = |marked| render_with(&frame(marked), 16, 16, RenderOptions::default()).unwrap();
    assert_eq!(drawn(true), drawn(false));
}

/// A flat layer whose effect reads its whole box draws the part of the box
/// off the canvas too: the frame shows what a canvas big enough for the
/// box shows there.
#[test]
fn an_effect_reading_its_whole_box_reads_the_part_off_the_canvas() {
    // A 16 by 32 box: red above, blue below, turned into rings about its
    // middle, so the red top row lands in the middle.
    let layer = |y: f64| {
        let half = |y0: f64, color: Color| {
            Node::Path(PathDraw {
                path: Rect::new(0.0, y0, 16.0, y0 + 16.0).to_path(0.1),
                paint: Paint::Solid(color),
                style: DrawStyle::Fill(FillRule::NonZero),
            })
        };
        Node::Layer(Layer {
            transform: Affine::translate((0.0, y)),
            effects: vec![
                Effect::new(
                    "polar-coordinates",
                    &[("interpolation", ParamValue::Number(1.0))],
                )
                .unwrap(),
            ],
            bounds: Some(Rect::new(0.0, 0.0, 16.0, 32.0)),
            ..Layer::new(vec![
                half(0.0, Color::rgba(1.0, 0.0, 0.0, 1.0)),
                half(16.0, Color::rgba(0.0, 0.0, 1.0, 1.0)),
            ])
        })
    };
    let frame = |y| Frame {
        nodes: vec![layer(y)],
        ..Frame::default()
    };
    // The whole box on a 16 by 32 canvas, and its lower half on a 16 by 16
    // one, the box's top 16 rows above the canvas.
    let whole = render_with(&frame(0.0), 16, 32, RenderOptions::GOLDEN).unwrap();
    let cut = render_with(&frame(-16.0), 16, 16, RenderOptions::GOLDEN).unwrap();
    for (x, y) in [(8, 0), (8, 1), (4, 6), (12, 9), (2, 14)] {
        let (a, b) = (cut.pixel(x, y).unwrap(), whole.pixel(x, y + 16).unwrap());
        assert!(near(a, b), "({x}, {y}): {a:?} against {b:?}");
    }
}
