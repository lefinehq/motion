use std::sync::Arc;

use vello_cpu::kurbo::{Affine, Circle, Rect, Shape};
use vello_cpu::peniko::{Blob, FontData};

use super::*;
use crate::blend::BLEND_MODES;
use crate::effect::Effect;
use crate::list::{
    Frame, Glyph, GlyphDraw, ImageDraw, Layer, LayerMask, MaskMode, Matte, MatteMode, Node,
    PathDraw,
};
use crate::picture::Picture;
use crate::render::{RenderOptions, drawn_at, subnormals_kept};
use crate::style::{Color, DrawStyle, FillRule, Paint};
use crate::{Image, memory, par};

const FONT: &[u8] = include_bytes!("../fixtures/FixtureSans-Black.ttf");

// The checks check: what differs is found.

#[test]
fn a_change_of_one_unit_in_the_last_place_is_found_anywhere() {
    let mut noise = Noise::new(7);
    let reference = noise.floats(257);
    assert_same_bits("itself", &reference, &reference.clone());
    for at in [0, 1, 128, 256] {
        for edge in edge_floats() {
            let mut reference = reference.clone();
            reference[at] = edge;
            let mut candidate = reference.clone();
            // One unit in the last place, or for a NaN one bit of payload.
            candidate[at] = f32::from_bits(edge.to_bits() ^ 1);
            let told = same_bits(&reference, &candidate).expect_err("a difference");
            assert!(told.contains(&format!("item {at}")), "{told}");
            assert!(told.starts_with("1 of 257"), "{told}");
        }
    }
}

#[test]
fn signed_zeros_and_nan_payloads_are_told_apart() {
    assert!(same_bits(&[0.0f32], &[-0.0]).is_err());
    assert!(same_bits(&[f32::NAN], &[f32::from_bits(0x7FC0_0001)]).is_err());
    assert!(same_bits(&[f32::NAN], &[f32::NAN]).is_ok());
    assert!(
        same_bits(
            &[[0.0f32, 1.0, 0.5, 1.0]],
            &[[0.0, 1.0, 0.5, 1.0f32.next_down()]]
        )
        .is_err()
    );
    assert!(same_bits(&[1u8, 2], &[1]).is_err());
    assert!(same_bits(&[1.0f64], &[1.0f64.next_up()]).is_err());
}

#[test]
fn a_write_past_either_end_is_found() {
    for len in WIDTHS.iter().copied().chain([0, 1000]) {
        let mut grid = Padded::new(len);
        assert!(grid.inside().iter().all(|v| v.to_bits() == JUNK.to_bits()));
        grid.inside_mut().fill(0.25);
        assert_eq!(grid.guards_whole(), Ok(()));
        let mut past = grid.clone();
        past.all_mut()[Padded::GUARD + len] = 0.0;
        assert!(
            past.guards_whole()
                .expect_err("found")
                .contains("1 past the end"),
            "{len}"
        );
        let mut before = grid.clone();
        before.all_mut()[Padded::GUARD - 1] = 0.0;
        assert!(
            before
                .guards_whole()
                .expect_err("found")
                .contains("1 before the start"),
            "{len}"
        );
    }
}

#[test]
fn the_edges_hold_every_kind_of_float_that_rounds_apart() {
    let edges = edge_floats();
    let has = |test: fn(f32) -> bool| edges.iter().any(|&v| test(v));
    assert!(has(|v| v == 0.0 && v.is_sign_negative()));
    assert!(has(f32::is_subnormal));
    assert!(has(|v| v.is_nan() && v.to_bits() & 0x003F_FFFF != 0));
    assert!(has(|v| v.is_nan() && v.is_sign_negative()));
    assert!(has(|v| v == f32::INFINITY));
    assert!(has(|v| v == f32::NEG_INFINITY));
    assert!(has(|v| v > 1.0 && v.is_finite()));
    // Either side of every half level.
    for k in 0..255u8 {
        let half = (f32::from(k) + 0.5) / 255.0;
        assert!(edges.contains(&half.next_down()) && edges.contains(&half.next_up()));
    }
    // A value no `f32` sum of small values holds exactly.
    assert!(has(|v| v == 16_777_217.0));
}

#[test]
fn noise_is_the_same_for_a_seed_and_reaches_every_kind() {
    let a = Noise::new(3).pixels(4096);
    assert_same_bits("one seed", &a, &Noise::new(3).pixels(4096));
    assert!(same_bits(&a, &Noise::new(4).pixels(4096)).is_err());
    assert!(a.iter().any(|p| p[3] == 0.0) && a.iter().any(|p| p[3] == 1.0));
    let floats = Noise::new(5).floats(4096);
    assert!(floats.iter().any(|v| v.is_nan()));
    assert!(floats.iter().any(|v| v.is_subnormal()));
    assert!(floats.iter().any(|v| *v > 1.0 && v.is_finite()));
}

#[test]
fn every_level_the_processor_has_is_there_fallback_first() {
    let levels = levels();
    assert!(levels[0].1.is_fallback());
    #[cfg(target_arch = "aarch64")]
    assert!(levels.iter().any(|(name, _)| *name == "neon"));
    #[cfg(target_arch = "x86_64")]
    assert!(levels.iter().any(|(name, _)| *name == "sse2"));
}

#[test]
fn subnormals_are_kept_on_the_threads_that_draw() {
    assert!(subnormals_kept());
    let kept = on_threads(4, || {
        rayon::broadcast(|_| subnormals_kept())
            .into_iter()
            .all(|kept| kept)
    });
    assert!(kept);
}

// What the checks hold the renderer to today: the same bits however the
// work is split and on however many threads, with grids clobbered.

#[test]
fn every_effect_gives_the_same_bits_however_its_work_is_split() {
    let space = crate::effect::Space {
        transform: Affine::translate((150.0, 100.0))
            * Affine::rotate(0.3)
            * Affine::scale(0.5)
            * Affine::translate((-120.0, -80.0)),
        bounds: Some(Rect::new(0.0, 0.0, 240.0, 160.0)),
        input: Some(crate::effect::Input::new(textured(), None)),
        time: 0.75,
        sound: None,
        outline: None,
        totals: Vec::new(),
        draft: false,
    };
    let apply = |effect: &Effect| {
        on_threads(3, || {
            let mut pixels = textured();
            effect.apply(&mut pixels, &space);
            pixels
        })
    };
    let sets = every_effect_set();
    let reference: Vec<_> = sets.iter().map(|(_, effect)| apply(effect)).collect();
    for split in [1, 7, 64, 333] {
        par::split_in_tests(Some(split));
        for ((what, effect), reference) in sets.iter().zip(&reference) {
            assert_same_bits(
                &format!("{what} split into tasks of {split}"),
                reference.data(),
                apply(effect).data(),
            );
        }
    }
    par::split_in_tests(None);
}

fn fill(path: impl Shape, rgba: [f32; 4]) -> Node {
    Node::Path(PathDraw {
        path: path.to_path(0.1),
        paint: Paint::Solid(Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3])),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// A 16 by 8 picture of ramps and partial alpha.
fn ramps() -> Picture {
    let mut bytes = Vec::new();
    for y in 0..8u8 {
        for x in 0..16u8 {
            bytes.extend([x * 16, y * 32, 255 - x * 8, 255 - (x * y) % 200]);
        }
    }
    Picture::from_image(&Image::new(16, 8, bytes).unwrap()).unwrap()
}

/// What each layer of [`sampler`] draws: shapes, a stretched picture and
/// a line of glyphs.
fn content() -> Vec<Node> {
    let font = FontData::new(Blob::new(Arc::new(FONT)), 0);
    vec![
        fill(Circle::new((60.0, 50.0), 34.0), [0.9, 0.3, 0.1, 0.9]),
        fill(Rect::new(70.0, 20.0, 150.0, 90.0), [0.1, 0.5, 0.9, 0.6]),
        Node::Image(ImageDraw {
            picture: ramps(),
            rect: Rect::new(20.0, 70.0, 100.0, 110.0),
        }),
        Node::Glyphs(GlyphDraw {
            font,
            size: 18.0,
            glyphs: [(55, 0), (76, 520), (58, 1584), (67, 2675)]
                .iter()
                .map(|&(id, advance)| Glyph {
                    id,
                    x: 90.0 + advance as f32 * 18.0 / 2000.0,
                    y: 108.0,
                })
                .collect(),
            glyph_transform: None,
            coords: Vec::new(),
            paint: Paint::Solid(Color::rgba(1.0, 1.0, 0.7, 1.0)),
            style: DrawStyle::Fill(FillRule::NonZero),
        }),
    ]
}

/// A frame with a layer for each family of work: blurs, light, maps that
/// move pixels, generated noise, tiles, colour, a matte, a feathered mask,
/// motion blur and the blend modes, at angles that sample between pixels.
fn sampler() -> Frame {
    let effect = |name: &str, params: &[(&str, crate::ParamValue)]| {
        Effect::new(name, params).expect("a known effect")
    };
    let number = crate::ParamValue::Number;
    let map: Arc<Node> = Arc::new(Node::Layer(Layer {
        effects: vec![effect("gaussian-blur", &[("blurriness", number(9.0))])],
        ..Layer::new(vec![fill(
            Circle::new((80.0, 60.0), 40.0),
            [1.0, 0.6, 0.2, 1.0],
        )])
    }));
    let families = vec![
        vec![effect("gaussian-blur", &[("blurriness", number(7.0))])],
        vec![effect("glow", &[("radius", number(8.0))])],
        vec![
            effect(
                "displacement-map",
                &[("horizontal", number(11.0)), ("vertical", number(-7.0))],
            )
            .with_input(Arc::clone(&map)),
        ],
        vec![effect("set-matte", &[]).with_input(Arc::clone(&map))],
        vec![effect("turbulent-displace", &[])],
        vec![effect("fractal-noise", &[])],
        vec![effect("motion-tile", &[])],
        vec![effect("levels", &[]), effect("curves", &[])],
        vec![effect("hue-saturation", &[]), effect("exposure", &[])],
        vec![effect("radial-blur", &[]), effect("drop-shadow", &[])],
        vec![effect("bevel", &[]), effect("sharpen", &[])],
        vec![effect("lens-distortion", &[]), effect("find-edges", &[])],
    ];
    let mut nodes: Vec<Node> = families
        .into_iter()
        .enumerate()
        .map(|(i, effects)| {
            let turn = 0.07 * i as f64 - 0.3;
            Node::Layer(Layer {
                transform: Affine::rotate_about(turn, (80.0, 60.0)),
                opacity: 0.55 + 0.03 * i as f32,
                blend: BLEND_MODES[(3 * i + 1) % BLEND_MODES.len()].mode,
                effects,
                ..Layer::new(content())
            })
        })
        .collect();
    nodes.push(Node::Layer(Layer {
        matte: Some(Matte {
            source: Box::new(fill(Circle::new((80.0, 60.0), 50.0), [1.0, 1.0, 1.0, 0.8])),
            mode: MatteMode::Luma,
        }),
        masks: vec![LayerMask {
            path: Rect::new(10.0, 10.0, 140.0, 100.0).to_path(0.1),
            mode: MaskMode::Add,
            inverted: false,
            opacity: 0.9,
            feather: [6.0, 3.0],
            expand: 1.5,
        }],
        motion: vec![
            Affine::translate((-3.0, 1.0)),
            Affine::IDENTITY,
            Affine::translate((4.0, -2.0)),
        ],
        ..Layer::new(content())
    }));
    Frame {
        background: Some(Color::rgba(0.08, 0.09, 0.12, 1.0)),
        nodes,
        ..Frame::default()
    }
}

#[test]
fn a_frame_is_the_same_at_every_thread_count_and_split_on_every_level() {
    let frame = sampler();
    for options in [RenderOptions::default(), RenderOptions::GOLDEN] {
        for (name, level) in levels() {
            let draw = || drawn_at(&frame, 160, 120, options, level, None).expect("drawn");
            let reference = on_threads(1, draw);
            // A frame of many colours, not a few flat ones.
            let colours: std::collections::HashSet<&[u8]> = reference.pixels().chunks(4).collect();
            assert!(colours.len() > 5000, "{} colours", colours.len());
            memory::set_clobber(true);
            for (threads, split) in [(2, None), (3, Some(7)), (8, Some(333)), (3, Some(1))] {
                par::split_in_tests(split);
                let drawn = on_threads(threads, draw);
                assert_same_bits(
                    &format!("{name}, {threads} threads, split {split:?}, {options:?}"),
                    reference.pixels(),
                    drawn.pixels(),
                );
            }
            par::split_in_tests(None);
            memory::set_clobber(false);
        }
    }
}

// The rasteriser's highp pack (K1): the bytes it was published with.

/// The bytes of a pixmap of `width` columns between guards of junk, after
/// `pack` packed `scratch` into `height` rows of it.
fn packed(
    width: usize,
    height: u16,
    scratch: &[f32],
    pack: impl FnOnce(&[f32], usize, &mut vello_cpu::region::Region<'_>),
) -> Vec<u8> {
    const GUARD: u16 = 5;
    let columns = u16::try_from(width).expect("a narrow tile row");
    let mut pixmap = vello_cpu::Pixmap::new(columns + 2 * GUARD, 4);
    pixmap.data_as_u8_slice_mut().fill(0xA5);
    {
        let mut target = pixmap.as_mut();
        let rect = vello_cpu::geometry::RectU16::new(GUARD, 0, GUARD + columns, height);
        let mut region = vello_cpu::region::Region::new(&mut target, rect);
        pack(scratch, width, &mut region);
    }
    pixmap.data_as_u8_slice().to_vec()
}

#[test]
fn the_rasterisers_pack_gives_the_bytes_it_was_published_with() {
    use vello_cpu::fine::{F32Kernel, FineKernel};
    let mut noise = Noise::new(11);
    let edges = edge_floats();
    for (name, level) in levels() {
        for &width in WIDTHS.iter().chain(&[63, 64, 65, 127, 128, 129, 300]) {
            for height in 1..=4 {
                let random = noise.floats(width * 16);
                let edge: Vec<f32> = edges.iter().copied().cycle().take(width * 16).collect();
                for scratch in [random, edge] {
                    let published = packed(width, height, &scratch, F32Kernel::pack_as_published);
                    // The guards are untouched, and so are the rows below.
                    let row = (width + 10) * 4;
                    assert!(published[..20].iter().all(|&b| b == 0xA5));
                    assert!(
                        published[usize::from(height) * row..]
                            .iter()
                            .all(|&b| b == 0xA5)
                    );
                    let changed = packed(width, height, &scratch, |scratch, width, region| {
                        fearless_simd::dispatch!(level, simd => {
                            <F32Kernel as FineKernel<_>>::pack(simd, scratch, width, region)
                        });
                    });
                    assert_same_bits(
                        &format!("{name}, {width} by {height}"),
                        &published,
                        &changed,
                    );
                }
            }
        }
    }
}

#[test]
fn the_rasterisers_pack_makes_the_published_byte_of_every_float() {
    use rayon::prelude::*;
    use vello_cpu::fine::F32Kernel;
    const RUN: u32 = 1 << 16;
    for (name, level) in levels() {
        (0..=u32::MAX / RUN).into_par_iter().for_each_init(
            || (vec![0; RUN as usize], vec![0; RUN as usize]),
            |(packed, published), run| {
                F32Kernel::packed_and_published(level, run * RUN, packed, published);
                if packed != published {
                    let at = (0..RUN as usize)
                        .find(|&i| packed[i] != published[i])
                        .unwrap();
                    let value = f32::from_bits(run * RUN + at as u32);
                    panic!(
                        "{name}: {value:?} ({:#010x}) packs to {}, published {}",
                        value.to_bits(),
                        packed[at],
                        published[at]
                    );
                }
            },
        );
    }
}
