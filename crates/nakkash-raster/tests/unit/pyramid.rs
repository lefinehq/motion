//! Unit tests for `src/pyramid.rs`.

use super::*;
use crate::blur;
use crate::oracle::{assert_same_bits, textured};

/// The largest and the mean difference of any channel between two grids.
fn error(a: &Pixels, b: &Pixels) -> (f32, f32) {
    let (mut most, mut sum) = (0.0f32, 0.0f64);
    for (p, q) in a.data().iter().zip(b.data()) {
        for c in 0..4 {
            let d = (p[c] - q[c]).abs();
            most = most.max(d);
            sum += f64::from(d);
        }
    }
    (most, (sum / (a.data().len() * 4) as f64) as f32)
}

#[test]
fn nothing_is_approximate_unless_asked() {
    assert!(!approximating());
    let source = textured();
    let large = Blur::gaussian([30.0, 30.0], Edges::Transparent);
    let mut exact = source.clone();
    blur::blur(&mut exact, &large);
    let mut again = source.clone();
    blur::blur(&mut again, &large);
    assert_same_bits("outside a draft", exact.data(), again.data());
    {
        let _off = approximate(false);
        let mut within = source.clone();
        blur::blur(&mut within, &large);
        assert_same_bits(
            "in a scope that is not a draft's",
            exact.data(),
            within.data(),
        );
    }
}

#[test]
fn small_blurs_stay_exact_in_a_draft() {
    let source = textured();
    for sigma in [0.5f32, 3.0, 8.0] {
        let small = Blur::gaussian([sigma, sigma], Edges::Repeat);
        let mut exact = source.clone();
        blur::blur(&mut exact, &small);
        let _on = approximate(true);
        let mut draft = source.clone();
        blur::blur(&mut draft, &small);
        assert_same_bits(&format!("sigma {sigma}"), exact.data(), draft.data());
    }
}

/// A textured grid of `width` by `height`, as [`textured`] draws its own.
fn textured_of(width: u16, height: u16) -> Pixels {
    let mut pixels = Pixels::transparent(width, height);
    let w = usize::from(width);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = ((i % w) as f32, (i / w) as f32);
        let a = (0.5 + 0.5 * (x * 0.05).sin() * (y * 0.07).cos()).clamp(0.0, 1.0);
        let c = |k: f32| (0.5 + 0.5 * (x * k + y * (0.1 - k)).sin()) * a;
        *p = [c(0.03), c(0.011), c(0.07), a];
    }
    pixels
}

#[test]
fn large_blurs_in_a_draft_are_close_to_exact() {
    // A grid of a preview's size, so the blur is small beside it.
    let source = textured_of(960, 540);
    for edges in [Edges::Transparent, Edges::Repeat] {
        for sigma in [40.0f32, 90.0, 200.0] {
            let large = Blur::gaussian([sigma, sigma * 0.8], edges);
            let mut exact = source.clone();
            blur::blur(&mut exact, &large);
            let mut draft = source.clone();
            let did = {
                let _on = approximate(true);
                pyramid_blur(&mut draft, &large)
            };
            assert!(did, "sigma {sigma} is worked out small");
            let (most, mean) = error(&exact, &draft);
            // Under half an 8-bit level on average, under 8 anywhere: the
            // most is at the grid's edges, where the copy repeats a block's
            // mean and the exact passes the edge pixel.
            assert!(mean < 0.5 / 255.0, "sigma {sigma}, {edges:?}: mean {mean}");
            assert!(most < 8.0 / 255.0, "sigma {sigma}, {edges:?}: most {most}");
        }
    }
}

fn pyramid_blur(pixels: &mut Pixels, b: &Blur) -> bool {
    super::blur(pixels, b)
}

/// A frame of one square with a blur of `blurriness` large enough for a
/// draft to work out small.
fn blurred_square() -> crate::list::Frame {
    use crate::effect::{Effect, ParamValue};
    use crate::list::{Frame, Layer, Node, PathDraw};
    use crate::style::{Color, DrawStyle, FillRule, Paint};
    use vello_cpu::kurbo::{Rect, Shape};
    let mut layer = Layer::new(vec![Node::Path(PathDraw {
        path: Rect::new(40.0, 30.0, 90.0, 70.0).to_path(0.1),
        paint: Paint::Solid(Color::WHITE),
        style: DrawStyle::Fill(FillRule::NonZero),
    })]);
    layer.effects = vec![
        Effect::new("gaussian-blur", &[("blurriness", ParamValue::Number(80.0))])
            .expect("a known effect"),
    ];
    Frame {
        nodes: vec![Node::Layer(layer)],
        ..Frame::default()
    }
}

#[test]
fn a_precise_or_fast_frame_never_approximates_whatever_its_thread_did() {
    use crate::render::{Quality, RenderOptions, render_with};
    let frame = blurred_square();
    for quality in [Quality::Precise, Quality::Fast] {
        let options = RenderOptions {
            quality,
            ..RenderOptions::default()
        };
        let want = render_with(&frame, 160, 100, options).unwrap();
        let got = {
            let _on = approximate(true);
            render_with(&frame, 160, 100, options).unwrap()
        };
        assert_eq!(want.pixels(), got.pixels(), "{quality:?}");
        assert!(!approximating());
    }
    // A draft frame does approximate, and comes close.
    let draft = RenderOptions {
        quality: Quality::Draft,
        ..RenderOptions::default()
    };
    let fast = RenderOptions {
        quality: Quality::Fast,
        ..RenderOptions::default()
    };
    let (a, b) = (
        render_with(&frame, 160, 100, fast).unwrap(),
        render_with(&frame, 160, 100, draft).unwrap(),
    );
    assert_ne!(a.pixels(), b.pixels(), "a draft works its blur out small");
    let most = a
        .pixels()
        .iter()
        .zip(b.pixels())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap();
    assert!(most <= 2, "a draft within 2 levels: {most}");
}
