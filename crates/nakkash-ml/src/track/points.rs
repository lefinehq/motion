//! Choosing points worth following: corners by the smallest eigenvalue of
//! the structure tensor, inside the object's mask.

use super::mask::Mask;
use crate::image::{Filter, Plane, Rgb8};

/// The size, in px a side, of the square picture points are chosen on.
pub const CHOICE_SIZE: usize = 512;
/// The most points chosen at once.
pub const MAX_POINTS: usize = 64;
/// How close, in px of the choice picture, two chosen points may be.
pub const MIN_APART: usize = 12;
/// The width, in px of the choice picture, of the margin where no point is
/// chosen.
pub const BORDER: usize = 8;
/// The half-width of the window the structure tensor sums over (7×7).
const WINDOW: usize = 3;

/// Up to [`MAX_POINTS`] corners of `frame` inside `mask`, strongest first,
/// at least [`MIN_APART`] px apart and [`BORDER`] px from the edges, found
/// on the frame and mask resized to [`CHOICE_SIZE`]², and returned in the
/// frame's pixels.
pub fn choose(frame: &Rgb8, mask: &Mask) -> Vec<[f64; 2]> {
    let s = CHOICE_SIZE;
    let grey = frame.resized(s, s, Filter::Triangle).grey();
    let inside = mask.resized(s, s);
    let mut strength = min_eigenvalue(&grey);
    for (k, value) in strength.iter_mut().enumerate() {
        let (x, y) = (k % s, k / s);
        let edge = x < BORDER || y < BORDER || x >= s - BORDER || y >= s - BORDER;
        if edge || !inside.inside[k] {
            *value = 0.0;
        }
    }
    let mut order: Vec<usize> = (0..s * s).filter(|&k| strength[k] > 0.0).collect();
    // Strongest first; equal strengths in raster order, so the choice is
    // the same on every run.
    order.sort_by(|&a, &b| strength[b].total_cmp(&strength[a]).then(a.cmp(&b)));
    let mut taken = vec![false; s * s];
    let mut chosen = Vec::new();
    for k in order {
        if chosen.len() == MAX_POINTS {
            break;
        }
        if taken[k] {
            continue;
        }
        let (x, y) = (k % s, k / s);
        chosen.push([
            (x as f64 + 0.5) * frame.width as f64 / s as f64 - 0.5,
            (y as f64 + 0.5) * frame.height as f64 / s as f64 - 0.5,
        ]);
        for yy in y.saturating_sub(MIN_APART)..(y + MIN_APART + 1).min(s) {
            for xx in x.saturating_sub(MIN_APART)..(x + MIN_APART + 1).min(s) {
                taken[yy * s + xx] = true;
            }
        }
    }
    chosen
}

/// The smallest eigenvalue of the structure tensor at each pixel: central
/// differences, their products summed over a 7×7 window.
fn min_eigenvalue(grey: &Plane) -> Vec<f32> {
    let (w, h) = (grey.width, grey.height);
    let at = |x: usize, y: usize| grey.data[y * w + x];
    let mut dx = vec![0f32; w * h];
    let mut dy = vec![0f32; w * h];
    for y in 0..h {
        for x in 1..w.saturating_sub(1) {
            dx[y * w + x] = at(x + 1, y) - at(x - 1, y);
        }
    }
    for y in 1..h.saturating_sub(1) {
        for x in 0..w {
            dy[y * w + x] = at(x, y + 1) - at(x, y - 1);
        }
    }
    let product = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(p, q)| p * q).collect::<Vec<f32>>();
    let a = box_sum(&product(&dx, &dx), w, h);
    let b = box_sum(&product(&dx, &dy), w, h);
    let c = box_sum(&product(&dy, &dy), w, h);
    (0..w * h)
        .map(|k| {
            let half = (a[k] - c[k]) / 2.0;
            (a[k] + c[k]) / 2.0 - (half * half + b[k] * b[k]).sqrt()
        })
        .collect()
}

/// Sums of `values` over the window around each pixel, cut at the edges.
fn box_sum(values: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut rows = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (x.saturating_sub(WINDOW), (x + WINDOW).min(w - 1));
            rows[y * w + x] = values[y * w + a..=y * w + b].iter().sum();
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        let (a, b) = (y.saturating_sub(WINDOW), (y + WINDOW).min(h - 1));
        for x in 0..w {
            out[y * w + x] = (a..=b).map(|j| rows[j * w + x]).sum();
        }
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/track/points.rs"]
mod tests;
