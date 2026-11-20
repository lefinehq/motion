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

/// A space drawn in a draft frame, or not.
fn space(draft: bool) -> crate::effect::Space {
    crate::effect::Space {
        draft,
        ..crate::effect::Space::default()
    }
}

#[test]
fn an_effect_approximates_only_in_a_draft() {
    use crate::effect::ParamValue;
    let source = textured();
    let large = Blur::gaussian([30.0, 30.0], Edges::Transparent);
    let mut exact = source.clone();
    blur::blur(&mut exact, &large);
    // Blurriness 100 is a standard deviation of 30 px.
    let values = [
        ParamValue::Number(100.0),
        ParamValue::Choice(0),
        ParamValue::Choice(0),
    ];
    let mut not_draft = source.clone();
    blur::gaussian_blur(&mut not_draft, &values, &space(false));
    assert_same_bits("outside a draft", exact.data(), not_draft.data());
    let mut draft = source.clone();
    blur::gaussian_blur(&mut draft, &values, &space(true));
    assert_ne!(exact.data(), draft.data(), "a draft works it out small");
}

#[test]
fn small_blurs_stay_exact_in_a_draft() {
    use crate::effect::ParamValue;
    let source = textured();
    for blurriness in [1.5f32, 10.0, 26.0] {
        let values = [
            ParamValue::Number(blurriness),
            ParamValue::Choice(0),
            ParamValue::Choice(1),
        ];
        let mut exact = source.clone();
        blur::gaussian_blur(&mut exact, &values, &space(false));
        let mut draft = source.clone();
        blur::gaussian_blur(&mut draft, &values, &space(true));
        let what = format!("blurriness {blurriness}");
        assert_same_bits(&what, exact.data(), draft.data());
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
            let did = pyramid_blur(&mut draft, &large);
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
fn only_a_draft_frame_approximates_whatever_else_its_threads_draw() {
    use crate::render::{Quality, RenderOptions, render_with};
    let frame = blurred_square();
    let at = |quality| RenderOptions {
        quality,
        ..RenderOptions::default()
    };
    let precise = render_with(&frame, 160, 100, at(Quality::Precise)).unwrap();
    let fast = render_with(&frame, 160, 100, at(Quality::Fast)).unwrap();
    let draft = render_with(&frame, 160, 100, at(Quality::Draft)).unwrap();
    // Draft and precise frames drawn side by side on one pool, whose
    // threads take each other's work: every precise frame keeps its bits.
    crate::oracle::on_threads(4, || {
        for _ in 0..8 {
            let (a, b) = rayon::join(
                || render_with(&frame, 160, 100, at(Quality::Draft)).unwrap(),
                || render_with(&frame, 160, 100, at(Quality::Precise)).unwrap(),
            );
            assert_eq!(a.pixels(), draft.pixels(), "the draft, beside a precise frame");
            assert_eq!(b.pixels(), precise.pixels(), "precise, beside a draft");
        }
    });
    // A draft does approximate, and comes close to fast.
    assert_ne!(fast.pixels(), draft.pixels(), "a draft works its blur out small");
    let most = fast
        .pixels()
        .iter()
        .zip(draft.pixels())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap();
    assert!(most <= 2, "a draft within 2 levels: {most}");
}
