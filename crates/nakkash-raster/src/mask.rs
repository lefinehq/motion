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
use crate::par;
use crate::pixels::Pixels;

/// The Gaussian's standard deviation per px of feather: an edge feathered
/// by `f` fades over about `f`, half inside the outline and half outside.
pub const SIGMA_PER_FEATHER: f32 = 0.3;

/// How much of a layer each pixel of the frame keeps, 0 to 1, row by row:
/// the masks combined in order. `None` when no mask cuts, because every
/// one's mode is `none`. `fill` draws one outline, in coordinates
/// `transform` maps to the frame, and gives its coverage. The painter
/// draws and combines them one at a time instead (`Painter::cut`).
#[cfg(test)]
pub(crate) fn coverage(
    masks: &[LayerMask],
    transform: Affine,
    width: u16,
    height: u16,
    fill: impl Fn(&LayerMask) -> Vec<f32>,
) -> Option<Vec<f32>> {
    let (start, steps) = steps(masks, transform)?;
    let mut kept = vec![start; usize::from(width) * usize::from(height)];
    for (mask, step) in steps {
        let own = fill(mask);
        apply(&mut kept, own, &step, width, height);
    }
    Some(kept)
}

/// One mask's step in combining a layer's masks, its outline's coverage
/// aside: how far it grows, how it is feathered, and how it combines with
/// the masks before it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaskStep {
    /// How many px its coverage grows by, or shrinks by when negative: a
    /// whole number, 0 for neither.
    pub grow: f32,
    /// The standard deviation, in px along x and y, of the Gaussian it is
    /// feathered by; 0 for none.
    pub sigma: [f32; 2],
    /// Whether it keeps what is outside its outline.
    pub inverted: bool,
    /// What its coverage is multiplied by.
    pub opacity: f32,
    /// How it combines with the masks before it.
    pub mode: MaskMode,
}

/// A mask whose outline the rasteriser drew, for an accelerator to
/// combine ([`Accelerator::mask`](crate::Accelerator::mask)).
#[derive(Debug, Clone, Copy)]
pub struct MaskCoverage<'a> {
    /// The outline filled in white, as the rasteriser drew it on a canvas
    /// of the frame: premultiplied RGBA, 8 bits a channel, row by row.
    pub drawn: &'a [u8],
    /// What is done with it.
    pub step: MaskStep,
}

/// The coverage the masks start from, and each cutting mask with its step;
/// `None` when no mask cuts.
pub(crate) fn steps(
    masks: &[LayerMask],
    transform: Affine,
) -> Option<(f32, Vec<(&LayerMask, MaskStep)>)> {
    let first = masks.iter().find(|m| m.mode != MaskMode::None)?;
    let start = match first.mode {
        MaskMode::Add | MaskMode::Lighten | MaskMode::Difference => 0.0,
        MaskMode::Subtract | MaskMode::Intersect | MaskMode::Darken | MaskMode::None => 1.0,
    };
    let [sx, sy] = scales(transform);
    let steps = masks
        .iter()
        .filter(|m| m.mode != MaskMode::None)
        .map(|mask| {
            let grow = mask.expand * (sx * sy).sqrt();
            let step = MaskStep {
                grow: match grow.abs() >= 0.5 {
                    true => grow.round(),
                    false => 0.0,
                },
                sigma: [mask.feather[0] * sx, mask.feather[1] * sy].map(|f| f * SIGMA_PER_FEATHER),
                inverted: mask.inverted,
                opacity: mask.opacity,
                mode: mask.mode,
            };
            (mask, step)
        })
        .collect();
    Some((start, steps))
}

/// `kept`, the masks' coverage so far, combined on the CPU with one more
/// mask the rasteriser drew: the reference an accelerator's
/// ([`Accelerator::mask`](crate::Accelerator::mask)) is held to.
pub fn combine_on_cpu(kept: &mut [f32], mask: &MaskCoverage<'_>, width: u16, height: u16) {
    let own = mask
        .drawn
        .chunks_exact(4)
        .map(|p| f32::from(p[3]) / 255.0)
        .collect();
    apply(kept, own, &mask.step, width, height);
}

/// `kept`, the coverage so far, combined with one mask's own coverage
/// `own` by its step.
fn apply(kept: &mut [f32], mut own: Vec<f32>, step: &MaskStep, width: u16, height: u16) {
    if step.grow != 0.0 {
        spread(&mut own, width, height, step.grow);
    }
    if step.sigma.iter().any(|s| *s > 0.0) {
        own = feathered(own, width, height, step.sigma);
    }
    par::each_with(kept, &own, |_, a, &m| {
        let m = match step.inverted {
            true => 1.0 - m,
            false => m,
        } * step.opacity;
        *a = combine(*a, m, step.mode);
    });
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
    par::each_with(pixels.data_mut(), &coverage, |_, pixel, &c| *pixel = [c; 4]);
    gaussian(&mut pixels, sigma, Edges::Transparent);
    par::map(pixels.data(), |_, p| p[3].clamp(0.0, 1.0))
}

/// Grows `coverage` by `by` px, or shrinks it when `by` is negative: each
/// pixel takes the largest (or smallest) coverage within a square of that
/// radius, one way at a time.
fn spread(coverage: &mut [f32], width: u16, height: u16, by: f32) {
    let (w, h) = (usize::from(width), usize::from(height));
    let radius = by.abs() as usize;
    let grow = by > 0.0;
    let line = |out: &mut [f32], line: &mut Vec<f32>| {
        line.clear();
        line.extend_from_slice(out);
        window(line, radius, grow, out);
    };
    par::rows(coverage, w, Vec::new, |_, row, room| line(row, room));
    par::columns(coverage, w, h, Vec::new, line);
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
