//! Lighting: effects that shade a layer as if it had depth. `bevel` raises
//! the layer's alpha into a surface that slopes up from its outline and
//! lights it from one side.
//!
//! The surface is worked out on the frame's grid from the layer's alpha:
//! how far each pixel lies inside its outline, by an exact Euclidean
//! distance transform (Felzenszwalb and Huttenlocher, "Distance transforms
//! of sampled functions", 2012), rising over the bevel's thickness.

use crate::colour::{number, rgb};
use crate::effect::{ParamValue, Space};
use crate::pixels::Pixels;

/// Far beyond any grid: the squared distance of a pixel with no outline
/// near it.
const FAR: f64 = 1e20;

/// The squared distance of each sample of `f` to the nearest point at
/// which `f` is 0, along one line of the grid, where `f` is 0 or [`FAR`]:
/// the lower envelope of the parabolas rooted at each sample.
fn line(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    let mut roots = vec![0usize; n];
    let mut bounds = vec![0.0f64; n + 1];
    let mut k = 0;
    bounds[0] = f64::NEG_INFINITY;
    bounds[1] = f64::INFINITY;
    for q in 1..n {
        let meet = |p: usize| {
            ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64))
        };
        let mut s = meet(roots[k]);
        while s <= bounds[k] {
            k -= 1;
            s = meet(roots[k]);
        }
        k += 1;
        roots[k] = q;
        bounds[k] = s;
        bounds[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, d) in out.iter_mut().enumerate() {
        while bounds[k + 1] < q as f64 {
            k += 1;
        }
        let p = roots[k];
        *d = (q as f64 - p as f64).powi(2) + f[p];
    }
}

/// How far, in pixels of the grid, each pixel lies inside the outline
/// where alpha crosses one half; 0 outside it.
fn depth(pixels: &Pixels) -> Vec<f64> {
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let inside: Vec<bool> = pixels.data().iter().map(|p| p[3] >= 0.5).collect();
    let mut squared: Vec<f64> = inside.iter().map(|&i| if i { FAR } else { 0.0 }).collect();
    let mut column = vec![0.0; height];
    let mut done = vec![0.0; height];
    for x in 0..width {
        for y in 0..height {
            column[y] = squared[y * width + x];
        }
        line(&column, &mut done);
        for y in 0..height {
            squared[y * width + x] = done[y];
        }
    }
    let mut row = vec![0.0; width];
    for y in 0..height {
        line(&squared[y * width..(y + 1) * width], &mut row);
        squared[y * width..(y + 1) * width].copy_from_slice(&row);
    }
    // Half a pixel less: the outline runs between the last pixel inside
    // and the first outside.
    squared
        .iter()
        .zip(&inside)
        .map(|(d, &i)| if i { (d.sqrt() - 0.5).max(0.0) } else { 0.0 })
        .collect()
}

/// `bevel`: the layer's alpha raised into a surface that rises from its
/// outline over `thickness` px of the layer, flat beyond, lit by light of
/// `color` coming from `angle` degrees clockwise from straight up. A
/// slope facing the light goes towards the light's colour, one facing
/// away towards black, each by `intensity` times how squarely it faces
/// it; flat parts keep their colour, and alpha stays.
pub(crate) fn bevel(pixels: &mut Pixels, values: &[ParamValue], space: &Space) {
    let scale = space.transform.determinant().abs().sqrt();
    let thickness = f64::from(number(values, 0)) * scale;
    let intensity = number(values, 3);
    if thickness <= 0.0 || intensity <= 0.0 {
        return;
    }
    let (sin, cos) = f64::from(number(values, 1)).to_radians().sin_cos();
    // Towards the light, on the grid: y grows downwards.
    let toward = [sin, -cos];
    let light = rgb(values, 2);
    let (width, height) = (usize::from(pixels.width()), usize::from(pixels.height()));
    let depth = depth(pixels);
    let rise = |x: usize, y: usize| (depth[y * width + x] / thickness).min(1.0);
    for y in 0..height {
        for x in 0..width {
            let p = &mut pixels.data_mut()[y * width + x];
            let a = p[3];
            if a <= 0.0 {
                continue;
            }
            // The slope across the surface, downhill, in rises per
            // thickness: 1 on a bevel that rises its full height across
            // its thickness.
            let at = |dx: isize, dy: isize| {
                let nx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                let ny = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                rise(nx, ny)
            };
            let downhill = [
                -(at(1, 0) - at(-1, 0)) / 2.0 * thickness,
                -(at(0, 1) - at(0, -1)) / 2.0 * thickness,
            ];
            let facing = (downhill[0] * toward[0] + downhill[1] * toward[1]).clamp(-1.0, 1.0);
            let k = (facing as f32) * intensity;
            for c in 0..3 {
                p[c] = match k >= 0.0 {
                    true => p[c] + (light[c] * a - p[c]) * k,
                    false => p[c] * (1.0 + k),
                };
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/light.rs"]
mod tests;
