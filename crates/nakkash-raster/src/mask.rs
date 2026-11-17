//! A layer's masks as one coverage: each mask's outline filled on a canvas
//! of the frame, moved out or in by its expansion, faded by its feather,
//! turned inside out when it keeps the outside, scaled by its opacity, and
//! combined with the masks before it by its mode ([D55]).
//!
//! [D55]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d55-masks-are-items-that-cut-a-layers-content-before-its-effects

use std::collections::VecDeque;

use vello_cpu::kurbo::Affine;

use crate::blur::{Edges, gaussian};
use crate::list::{LayerMask, MaskMode};
use crate::pixels::Pixels;

/// The Gaussian's standard deviation per px of feather: an edge feathered
/// by `f` fades over about `f`, half inside the outline and half outside.
pub const SIGMA_PER_FEATHER: f32 = 0.3;

/// How much of a layer each pixel of the frame keeps, 0 to 1, row by row:
/// the masks combined in order. `None` when no mask cuts, because every
/// one's mode is `none`. `fill` draws one outline, in coordinates
/// `transform` maps to the frame, and gives its coverage.
pub(crate) fn coverage(
    masks: &[LayerMask],
    transform: Affine,
    width: u16,
    height: u16,
    fill: impl Fn(&LayerMask) -> Vec<f32>,
) -> Option<Vec<f32>> {
    let first = masks.iter().find(|m| m.mode != MaskMode::None)?;
    let start = match first.mode {
        MaskMode::Add | MaskMode::Lighten | MaskMode::Difference => 0.0,
        MaskMode::Subtract | MaskMode::Intersect | MaskMode::Darken | MaskMode::None => 1.0,
    };
    let [sx, sy] = scales(transform);
    let mut kept = vec![start; usize::from(width) * usize::from(height)];
    for mask in masks.iter().filter(|m| m.mode != MaskMode::None) {
        let mut own = fill(mask);
        let grow = mask.expand * (sx * sy).sqrt();
        if grow.abs() >= 0.5 {
            spread(&mut own, width, height, grow.round());
        }
        let sigma = [mask.feather[0] * sx, mask.feather[1] * sy].map(|f| f * SIGMA_PER_FEATHER);
        if sigma.iter().any(|s| *s > 0.0) {
            own = feathered(own, width, height, sigma);
        }
        for (a, m) in kept.iter_mut().zip(own) {
            let m = match mask.inverted {
                true => 1.0 - m,
                false => m,
            } * mask.opacity;
            *a = combine(*a, m, mask.mode);
        }
    }
    Some(kept)
}

/// What the coverage so far, `a`, becomes with a mask covering `m`.
fn combine(a: f32, m: f32, mode: MaskMode) -> f32 {
    match mode {
        MaskMode::Add => a + m - a * m,
        MaskMode::Subtract => a * (1.0 - m),
        MaskMode::Intersect => a * m,
        MaskMode::Lighten => a.max(m),
        MaskMode::Darken => a.min(m),
        MaskMode::Difference => (a - m).abs(),
        MaskMode::None => a,
    }
}

/// How many pixels one unit of x and of y spans under `transform`.
fn scales(transform: Affine) -> [f32; 2] {
    let [a, b, c, d, ..] = transform.as_coeffs();
    [a.hypot(b) as f32, c.hypot(d) as f32]
}

/// `coverage` blurred by a Gaussian of standard deviation `sigma` each way.
fn feathered(coverage: Vec<f32>, width: u16, height: u16, sigma: [f32; 2]) -> Vec<f32> {
    let mut pixels = Pixels::transparent(width, height);
    for (pixel, c) in pixels.data_mut().iter_mut().zip(coverage) {
        *pixel = [c; 4];
    }
    gaussian(&mut pixels, sigma, Edges::Transparent);
    pixels.data().iter().map(|p| p[3].clamp(0.0, 1.0)).collect()
}

/// Grows `coverage` by `by` px, or shrinks it when `by` is negative: each
/// pixel takes the largest (or smallest) coverage within a square of that
/// radius, one way at a time.
fn spread(coverage: &mut [f32], width: u16, height: u16, by: f32) {
    let (w, h) = (usize::from(width), usize::from(height));
    let radius = by.abs() as usize;
    let grow = by > 0.0;
    let mut line = Vec::with_capacity(w.max(h));
    for y in 0..h {
        line.clear();
        line.extend_from_slice(&coverage[y * w..(y + 1) * w]);
        window(&line, radius, grow, &mut coverage[y * w..(y + 1) * w]);
    }
    let mut out = vec![0.0; h];
    for x in 0..w {
        line.clear();
        line.extend((0..h).map(|y| coverage[y * w + x]));
        window(&line, radius, grow, &mut out);
        for (y, v) in out.iter().enumerate() {
            coverage[y * w + x] = *v;
        }
    }
}

/// Each of `out` the largest (`max`) or smallest of `line` within `radius`
/// of it, by a running window. Beyond the line the edge pixel repeats, so a
/// mask that runs past the frame is not shrunk at the frame's edge.
fn window(line: &[f32], radius: usize, max: bool, out: &mut [f32]) {
    let n = line.len();
    let beats = |a: f32, b: f32| if max { a >= b } else { a <= b };
    // Indices of the window whose values come in order, best first.
    let mut best: VecDeque<usize> = VecDeque::new();
    let mut next = 0;
    for (i, slot) in out.iter_mut().enumerate().take(n) {
        let last = (i + radius).min(n - 1);
        while next <= last {
            while best.back().is_some_and(|&j| beats(line[next], line[j])) {
                best.pop_back();
            }
            best.push_back(next);
            next += 1;
        }
        while best.front().is_some_and(|&j| j + radius < i) {
            best.pop_front();
        }
        *slot = line[*best.front().expect("the window holds pixel i")];
    }
}

#[cfg(test)]
#[path = "../tests/unit/mask.rs"]
mod tests;
