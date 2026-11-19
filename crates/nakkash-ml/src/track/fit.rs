//! Fits of one frame's points to a reference frame's: a homography by RANSAC
//! and a similarity on its inliers, and the guard that keeps a homography
//! only when the similarity agrees with it ([D88]).
//!
//! [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots

use super::linalg::{self, Mat3};

/// How many minimal sets RANSAC tries.
pub const RANSAC_ROUNDS: usize = 300;
/// How far, in px, a point may land from where a homography sends it and
/// still count as its inlier.
pub const INLIER_PX: f64 = 3.0;
/// The fewest inliers a homography needs to be used for the corners.
pub const GUARD_INLIERS: usize = 8;
/// How far, as a share of the reference diagonal scaled by the similarity,
/// a homography's corner may lie from the similarity's.
pub const GUARD_REACH: f64 = 0.5;
/// The seed of RANSAC's choices, so a fit is the same on every run.
const SEED: u64 = 0x6e61_6b6b_6173_6801;

/// A similarity: `x ↦ scale · R(rotate) · x + shift`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Similarity {
    /// Its scale.
    pub scale: f64,
    /// Its rotation in radians, clockwise on screen (y down).
    pub angle: f64,
    /// Its translation.
    pub shift: [f64; 2],
}

impl Similarity {
    /// The identity.
    pub const IDENTITY: Similarity = Similarity {
        scale: 1.0,
        angle: 0.0,
        shift: [0.0, 0.0],
    };

    /// `p` through it.
    pub fn apply(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let (s, c) = self.angle.sin_cos();
        [
            self.scale * (c * x - s * y) + self.shift[0],
            self.scale * (s * x + c * y) + self.shift[1],
        ]
    }

    /// It after `first`: `self ∘ first`.
    pub fn after(&self, first: &Similarity) -> Similarity {
        Similarity {
            scale: self.scale * first.scale,
            angle: self.angle + first.angle,
            shift: self.apply(first.shift),
        }
    }

    /// The least-squares similarity taking `from` onto `to`, as a rotation
    /// (never a reflection); `None` for fewer than two distinct points.
    ///
    /// In complex numbers it is `b ≈ c·a + t` with `c = Σ ā·b / Σ |a|²` over
    /// the centred points, which is the closed form of the least-squares
    /// rotation-and-scale.
    pub fn fit(from: &[[f64; 2]], to: &[[f64; 2]]) -> Option<Similarity> {
        let n = from.len().min(to.len());
        if n < 2 {
            return None;
        }
        let mean = |p: &[[f64; 2]]| {
            let s = p[..n]
                .iter()
                .fold([0.0, 0.0], |a, q| [a[0] + q[0], a[1] + q[1]]);
            [s[0] / n as f64, s[1] / n as f64]
        };
        let (ma, mb) = (mean(from), mean(to));
        let (mut re, mut im, mut norm) = (0.0, 0.0, 0.0);
        for (a, b) in from[..n].iter().zip(&to[..n]) {
            let (ax, ay) = (a[0] - ma[0], a[1] - ma[1]);
            let (bx, by) = (b[0] - mb[0], b[1] - mb[1]);
            re += ax * bx + ay * by;
            im += ax * by - ay * bx;
            norm += ax * ax + ay * ay;
        }
        if norm < 1e-12 {
            return None;
        }
        let scale = (re * re + im * im).sqrt() / norm;
        let angle = im.atan2(re);
        let turned = Similarity {
            scale,
            angle,
            shift: [0.0, 0.0],
        }
        .apply(ma);
        Some(Similarity {
            scale,
            angle,
            shift: [mb[0] - turned[0], mb[1] - turned[1]],
        })
    }
}

/// The homography taking `from` onto `to` in the least-squares sense of the
/// direct linear transform, with both sets normalised first (centred, mean
/// distance √2); `None` for fewer than four points or a degenerate set.
pub fn homography(from: &[[f64; 2]], to: &[[f64; 2]]) -> Option<Mat3> {
    let n = from.len().min(to.len());
    if n < 4 {
        return None;
    }
    let (ta, a) = normalise(&from[..n])?;
    let (tb, b) = normalise(&to[..n])?;
    // AᵀA of the 2n×9 system, accumulated row by row.
    let mut ata = [0.0; 81];
    for (p, q) in a.iter().zip(&b) {
        let (x, y, u, v) = (p[0], p[1], q[0], q[1]);
        let rows = [
            [-x, -y, -1.0, 0.0, 0.0, 0.0, u * x, u * y, u],
            [0.0, 0.0, 0.0, -x, -y, -1.0, v * x, v * y, v],
        ];
        for row in &rows {
            for i in 0..9 {
                for j in 0..9 {
                    ata[i * 9 + j] += row[i] * row[j];
                }
            }
        }
    }
    let (values, vectors) = linalg::symmetric_eigen(&ata, 9);
    // A set whose two smallest values are both near zero has a family of
    // solutions: three of its points are collinear.
    let largest = values[8].abs().max(1e-300);
    if values[1].abs() / largest < 1e-12 {
        return None;
    }
    let h: Vec<f64> = (0..9).map(|k| vectors[k * 9]).collect();
    let hn = [[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], h[8]]];
    let full = linalg::mul(&linalg::inverse(&tb)?, &linalg::mul(&hn, &ta));
    let w = full[2][2];
    if w.abs() < 1e-12 || !w.is_finite() {
        return None;
    }
    Some(full.map(|row| row.map(|v| v / w)))
}

/// `points` centred on their mean and scaled to a mean distance of √2 from
/// it, with the map that does it; `None` when they all coincide.
fn normalise(points: &[[f64; 2]]) -> Option<(Mat3, Vec<[f64; 2]>)> {
    let n = points.len() as f64;
    let m = points
        .iter()
        .fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
    let spread = points
        .iter()
        .map(|p| ((p[0] - m[0]).powi(2) + (p[1] - m[1]).powi(2)).sqrt())
        .sum::<f64>()
        / n;
    if spread < 1e-12 {
        return None;
    }
    let k = std::f64::consts::SQRT_2 / spread;
    let t = [[k, 0.0, -k * m[0]], [0.0, k, -k * m[1]], [0.0, 0.0, 1.0]];
    Some((
        t,
        points
            .iter()
            .map(|p| [k * (p[0] - m[0]), k * (p[1] - m[1])])
            .collect(),
    ))
}

/// A small, fast, seeded generator (SplitMix64): RANSAC's choices are the
/// same on every machine and every run.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator from `seed`.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `n`, without the bias of a plain remainder.
    pub fn below(&mut self, n: usize) -> usize {
        let n = n as u64;
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let r = self.next_u64();
            if r < zone {
                return (r % n) as usize;
            }
        }
    }
}

/// The homography RANSAC finds from `from` onto `to`, refitted on its
/// inliers, and which points are its inliers; `None` with no inliers when
/// there are fewer than four points or every minimal set is degenerate.
pub fn ransac_homography(from: &[[f64; 2]], to: &[[f64; 2]]) -> (Option<Mat3>, Vec<bool>) {
    let n = from.len().min(to.len());
    if n < 4 {
        return (None, vec![false; n]);
    }
    let mut rng = Rng::new(SEED);
    let mut best: Option<(Mat3, Vec<bool>, usize)> = None;
    for _ in 0..RANSAC_ROUNDS {
        let mut pick = [0usize; 4];
        let mut k = 0;
        while k < 4 {
            let i = rng.below(n);
            if !pick[..k].contains(&i) {
                pick[k] = i;
                k += 1;
            }
        }
        let a: Vec<[f64; 2]> = pick.iter().map(|&i| from[i]).collect();
        let b: Vec<[f64; 2]> = pick.iter().map(|&i| to[i]).collect();
        let Some(h) = homography(&a, &b) else {
            continue;
        };
        let inliers = inliers(&h, &from[..n], &to[..n]);
        let count = inliers.iter().filter(|&&i| i).count();
        if best.as_ref().is_none_or(|(_, _, c)| count > *c) {
            best = Some((h, inliers, count));
        }
    }
    let Some((h, inl, count)) = best else {
        return (None, vec![false; n]);
    };
    if count < 4 {
        return (Some(h), inl);
    }
    let a: Vec<[f64; 2]> = (0..n).filter(|&i| inl[i]).map(|i| from[i]).collect();
    let b: Vec<[f64; 2]> = (0..n).filter(|&i| inl[i]).map(|i| to[i]).collect();
    match homography(&a, &b) {
        Some(refit) => {
            let again = inliers(&refit, &from[..n], &to[..n]);
            (Some(refit), again)
        }
        None => (Some(h), inl),
    }
}

/// Which points `h` sends within [`INLIER_PX`] of where they went.
fn inliers(h: &Mat3, from: &[[f64; 2]], to: &[[f64; 2]]) -> Vec<bool> {
    from.iter()
        .zip(to)
        .map(|(a, b)| linalg::apply(h, *a).is_some_and(|p| distance(p, *b) < INLIER_PX))
        .collect()
}

/// The distance between two points.
pub fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// One frame's fit against a reference frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameFit {
    /// The similarity from the reference frame.
    pub similarity: Similarity,
    /// The reference corners on this frame: through the homography when
    /// the guard passed, else through the similarity.
    pub corners: [[f64; 2]; 4],
    /// Whether the guard refused the homography.
    pub guarded: bool,
    /// The homography's inliers.
    pub inliers: usize,
    /// The points seen on both frames.
    pub points: usize,
}

/// The fit of `to`, where the points are on this frame, against `from`,
/// where they are on the reference frame, carrying `corners` (on the
/// reference frame); `None` with fewer than three points.
pub fn fit_frame(from: &[[f64; 2]], to: &[[f64; 2]], corners: &[[f64; 2]; 4]) -> Option<FrameFit> {
    let n = from.len().min(to.len());
    if n < 3 {
        return None;
    }
    let (h, inl) = ransac_homography(&from[..n], &to[..n]);
    let count = inl.iter().filter(|&&i| i).count();
    let similarity = match count >= 3 {
        true => {
            let a: Vec<[f64; 2]> = (0..n).filter(|&i| inl[i]).map(|i| from[i]).collect();
            let b: Vec<[f64; 2]> = (0..n).filter(|&i| inl[i]).map(|i| to[i]).collect();
            Similarity::fit(&a, &b)
        }
        false => Similarity::fit(&from[..n], &to[..n]),
    }?;
    let by_similarity = corners.map(|c| similarity.apply(c));
    let reach = GUARD_REACH * distance(corners[0], corners[2]) * similarity.scale;
    let by_homography = h.and_then(|h| {
        let mapped: Vec<[f64; 2]> = corners
            .iter()
            .filter_map(|c| linalg::apply(&h, *c))
            .collect();
        <[[f64; 2]; 4]>::try_from(mapped).ok()
    });
    let kept = by_homography.filter(|hc| {
        count >= GUARD_INLIERS
            && hc
                .iter()
                .zip(&by_similarity)
                .all(|(a, b)| distance(*a, *b) < reach)
    });
    Some(FrameFit {
        similarity,
        corners: kept.unwrap_or(by_similarity),
        guarded: kept.is_none(),
        inliers: count,
        points: n,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/track/fit.rs"]
mod tests;
