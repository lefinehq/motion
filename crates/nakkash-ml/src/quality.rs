//! The measures the gates of [D86] read: accuracy against ground truth,
//! paired with how the error changes between frames, so a blurred matte
//! cannot pass.
//!
//! Alphas are planes of values from 0 to 1 at the clip's size.
//!
//! [D86]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d86-bakes-are-gated-by-measured-quality-on-fixed-shots

use crate::image::Plane;

/// The intersection over union of two alphas thresholded at 0.5; 1 when
/// both are empty.
pub fn iou(a: &Plane, b: &Plane) -> f64 {
    let (mut both, mut either) = (0u64, 0u64);
    for (x, y) in a.data.iter().zip(&b.data) {
        let (x, y) = (*x > 0.5, *y > 0.5);
        both += u64::from(x && y);
        either += u64::from(x || y);
    }
    match either {
        0 => 1.0,
        _ => both as f64 / either as f64,
    }
}

/// The sum of absolute differences, in thousands, as matting benchmarks
/// report it.
pub fn sad(a: &Plane, b: &Plane) -> f64 {
    a.data
        .iter()
        .zip(&b.data)
        .map(|(x, y)| f64::from((x - y).abs()))
        .sum::<f64>()
        / 1000.0
}

/// The root mean square of the error of an alpha's change between frames,
/// against the ground truth's change, ×1000 (dtSSD), averaged over the
/// frame pairs.
pub fn dtssd(alphas: &[Plane], truth: &[Plane]) -> f64 {
    let pairs: Vec<f64> = (1..alphas.len().min(truth.len()))
        .map(|t| {
            let n = alphas[t].data.len() as f64;
            let sum: f64 = (0..alphas[t].data.len())
                .map(|i| {
                    let d = f64::from(alphas[t].data[i] - alphas[t - 1].data[i])
                        - f64::from(truth[t].data[i] - truth[t - 1].data[i]);
                    d * d
                })
                .sum();
            (sum / n).sqrt() * 1000.0
        })
        .collect();
    match pairs.len() {
        0 => 0.0,
        n => pairs.iter().sum::<f64>() / n as f64,
    }
}

#[cfg(test)]
#[path = "../tests/unit/quality.rs"]
mod tests;
