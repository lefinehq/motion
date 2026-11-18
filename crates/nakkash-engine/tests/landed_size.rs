//! A composition placed smaller than its own size is drawn at the size it
//! lands at, not drawn whole and then shrunk ([D76]). These tests hold that
//! rule to the reference tolerance: a scene drawn through a composition at
//! half its size against the same scene drawn whole and halved, by the
//! measure `nakkash diff` uses ([`DiffOptions::TOLERANCE`],
//! [`DiffOptions::PASSING`]).
//!
//! [D76]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d76-a-composition-is-drawn-at-the-size-it-lands-at

use nakkash_engine::{DiffOptions, Image, RenderOptions, Session};
use rstest::rstest;

/// The layers of a 400 by 200 scene whose effects measure lengths in its
/// own px: blurs, a glow, a shadow, noise and a displacement.
const LAYERS: &str = r##"[
  {"id":"plate","rect":"400x200","fill":"#1A2238","at":"center center",
   "fx":[{"fractal-noise":0,"complexity":4,"scale":60,"opacity":0.35}]},
  {"id":"orb","at":"120 100","shapes":[{"ellipse":[120,120]},{"fill":"#4F7BFF"}],
   "fx":[{"gaussian-blur":40}]},
  {"id":"card","rect":"160x90 r16","fill":"#F2F4FA","at":"260 100",
   "fx":[{"drop-shadow":12,"softness":30,"opacity":0.6},{"glow":20,"threshold":0.5,"intensity":0.8}]},
  {"id":"bar","rect":"300x14 r7","fill":"#FFB03A","at":"200 170",
   "fx":[{"turbulent-displace":30,"size":40},{"box-blur":4}]}
]"##;

/// The scene drawn whole.
fn whole() -> Session {
    format!(
        r##"{{"nakkash":1,"size":"400x200","fps":30,"dur":"1s","bg":"#000000","layers":{LAYERS}}}"##
    )
    .parse()
    .unwrap()
}

/// The scene as a composition placed at `scale` of its size in a frame
/// that size.
fn placed(scale: f64) -> Session {
    let (w, h) = (400.0 * scale, 200.0 * scale);
    format!(
        r##"{{"nakkash":1,"size":"{w}x{h}","fps":30,"dur":"1s","bg":"#000000",
        "comps":{{"whole":{{"size":"400x200","dur":"1s","layers":{LAYERS}}}}},
        "layers":[{{"id":"whole","comp":"whole","anchor":[200,100],"position":[{x},{y}],"scale":{scale}}}]}}"##,
        x = w / 2.0,
        y = h / 2.0,
    )
    .parse()
    .unwrap()
}

/// `image` shrunk by `by` each way, each pixel the mean of the `by` by `by`
/// it covers: what drawing it whole and laying it down smaller gives,
/// before a resampler's own filter.
fn shrunk(image: &Image, by: u32) -> Vec<[f64; 3]> {
    let (w, h) = (image.width() / by, image.height() / by);
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0; 3];
            for (dx, dy) in (0..by).flat_map(|dx| (0..by).map(move |dy| (dx, dy))) {
                let p = image.pixel(x * by + dx, y * by + dy).unwrap();
                for c in 0..3 {
                    sum[c] += f64::from(p[c]);
                }
            }
            out.push(sum.map(|s| s / f64::from(by * by)));
        }
    }
    out
}

#[rstest]
#[case::half(2)]
#[case::quarter(4)]
fn a_composition_drawn_at_the_size_it_lands_at_matches_it_drawn_whole(#[case] by: u32) {
    let limit = DiffOptions::TOLERANCE * 255.0;
    for frame in [0.0, 15.0] {
        let reference = shrunk(
            &whole().render_with(frame, RenderOptions::GOLDEN).unwrap(),
            by,
        );
        let landed = placed(1.0 / f64::from(by))
            .render_with(frame, RenderOptions::GOLDEN)
            .unwrap();
        assert_eq!(landed.pixels().len() / 4, reference.len());
        let (mut failing, mut sum) = (0usize, 0.0);
        for (ours, theirs) in landed.pixels().chunks_exact(4).zip(&reference) {
            let d = [0, 1, 2].map(|c| (f64::from(ours[c]) - theirs[c]).abs());
            failing += usize::from(d.iter().any(|d| *d > limit));
            sum += d.iter().sum::<f64>();
        }
        let share = failing as f64 / reference.len() as f64;
        let mean = sum / (reference.len() * 3) as f64 / 255.0;
        assert!(
            share <= DiffOptions::PASSING && mean <= DiffOptions::ERROR_MARGIN,
            "at 1/{by}, frame {frame}: {:.2} % failing, mean error {mean:.4}",
            share * 100.0
        );
    }
}
