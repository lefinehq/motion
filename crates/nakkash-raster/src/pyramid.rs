//! Large blurs worked out on a smaller copy: approximate, and drawn only
//! for [`Quality::Draft`] previews ([D93]).
//!
//! A blur whose passes reach at least [`LEAST`] px each way is worked out on
//! a copy shrunk by 2, 4 or 8 (block means), with passes of the same
//! variance at that scale less what the shrinking and the bilinear way back
//! up spread, and sampled back up. The exact passes ([`crate::blur`]) do
//! the work on the copy. Nothing here is reached by a render of any other
//! quality: [`approximating`] is only true inside a frame drawn at
//! [`Quality::Draft`].
//!
//! [`Quality::Draft`]: crate::Quality::Draft
//! [D93]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d93-glow-and-blurs-do-less-of-the-same-work

use std::cell::Cell;

use crate::blur::{Edges, along};
use crate::pixels::{Pixel, Pixels};
use crate::plan::{Blur, Passes};

/// The least radius per pass, in px, worked out on a smaller copy.
pub const LEAST: f32 = 16.0;

/// The least radius per pass left at the smaller scale: the copy is shrunk
/// only as far as keeps at least this.
const KEPT_RADIUS: f32 = 8.0;

/// The most the copy is shrunk by, as a power of 2.
const MOST_LEVELS: u32 = 3;

thread_local! {
    static APPROXIMATING: Cell<bool> = const { Cell::new(false) };
}

/// While alive, blurs on this thread may be approximate.
pub(crate) struct Approximating(bool);

impl Drop for Approximating {
    fn drop(&mut self) {
        APPROXIMATING.with(|a| a.set(self.0));
    }
}

/// Lets blurs on this thread be approximate until the guard goes, when
/// `on`; a render at [`crate::Quality::Draft`] holds one over its frame.
pub(crate) fn approximate(on: bool) -> Approximating {
    Approximating(APPROXIMATING.with(|a| a.replace(on)))
}

/// Whether blurs on this thread may be approximate.
pub(crate) fn approximating() -> bool {
    APPROXIMATING.with(Cell::get)
}

/// The variance of one extended box pass of radius `r`.
fn variance(r: f32) -> f64 {
    let (l, a) = (f64::from(r.floor()), f64::from(r.fract()));
    (l * (l + 1.0) * (2.0 * l + 1.0) / 3.0 + 2.0 * a * (l + 1.0) * (l + 1.0))
        / (2.0 * l + 1.0 + 2.0 * a)
}

/// The radius of one extended box pass of variance `v`.
fn radius(v: f64) -> f32 {
    let l = (((12.0 * v + 1.0).sqrt() - 1.0) / 2.0).floor().max(0.0);
    let a = ((2.0 * l + 1.0) * (v - l * (l + 1.0) / 3.0)) / (2.0 * ((l + 1.0) * (l + 1.0) - v));
    (l + a.clamp(0.0, 1.0)) as f32
}

/// Works `blur` out on a smaller copy of `pixels` and samples it back up,
/// when its passes are large enough each way; whether it did. Without that
/// the caller runs the exact passes.
pub(crate) fn blur(pixels: &mut Pixels, blur: &Blur) -> bool {
    let ways = [blur.rows, blur.columns];
    if ways.iter().any(|p| p.count == 0 || p.radius < LEAST) {
        return false;
    }
    let least = ways.iter().map(|p| p.radius).fold(f32::INFINITY, f32::min);
    let levels = ((least / KEPT_RADIUS).log2().floor().max(0.0) as u32).min(MOST_LEVELS);
    if levels == 0 {
        return false;
    }
    let by = 1usize << levels;
    let f = by as f64;
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let (small_w, small_h) = (width.div_ceil(by), height.div_ceil(by));
    let (Ok(sw), Ok(sh)) = (u16::try_from(small_w), u16::try_from(small_h)) else {
        return false;
    };
    // Block means: beyond the grid nothing, or the edge, as the blur's own
    // edges read.
    let source = pixels.data();
    let mut small = Pixels::made(sw, sh, |i| {
        let (bx, by_) = ((i % small_w) * by, (i / small_w) * by);
        let mut sum = [0.0f32; 4];
        for y in by_..by_ + by {
            for x in bx..bx + by {
                let p: Pixel = match (x < width && y < height, blur.edges) {
                    (true, _) => source[y * width + x],
                    (false, Edges::Transparent) => [0.0; 4],
                    (false, Edges::Repeat) => source[y.min(height - 1) * width + x.min(width - 1)],
                };
                for c in 0..4 {
                    sum[c] += p[c];
                }
            }
        }
        sum.map(|c| c / (by * by) as f32)
    });
    // The same variance at the smaller scale, less what the block means
    // and the bilinear way back spread: (f² − 1)/12 and f²/6 px² of the
    // grid's.
    let spread = (f * f - 1.0) / 12.0 + f * f / 6.0;
    for (way, Passes { radius: r, count }) in ways.into_iter().enumerate() {
        let total = variance(r) * count as f64 - spread;
        let per = (total / count as f64).max(0.0) / (f * f);
        along(&mut small, way, radius(per), count, blur.edges);
    }
    let small = &small;
    // Read back with the copy's edge pixels held: the copy already holds
    // what the edges bring in.
    *pixels = Pixels::made(pixels.width(), pixels.height(), |i| {
        let (x, y) = ((i % width) as f64 + 0.5, (i / width) as f64 + 0.5);
        small.sample_repeating(x / f, y / f)
    });
    true
}

#[cfg(test)]
#[path = "../tests/unit/pyramid.rs"]
mod tests;
