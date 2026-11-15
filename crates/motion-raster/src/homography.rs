//! Projective maps of the plane: the 3×3 matrices a projected layer is
//! laid onto the canvas through ([D58]).
//!
//! A point `(x, y)` maps to `(x', y', w)` = `M · (x, y, 1)`, and is seen at
//! `(x'/w, y'/w)`. For a plane seen through a camera, `w` is the depth of
//! the point in front of the camera, so it is positive where the plane is
//! in front and negative where it is behind. An affine map has `w = 1`
//! everywhere.
//!
//! [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera

use std::ops::Mul;

use vello_cpu::kurbo::{Affine, Point};

/// A projective map of the plane, a 3×3 matrix acting on `(x, y, 1)`.
///
/// Composing reads as [`Affine`] does: `a * b` maps by `b`, then by `a`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography {
    rows: [[f64; 3]; 3],
}

impl Homography {
    /// The map that moves nothing.
    pub const IDENTITY: Homography = Homography {
        rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    /// The map with these rows: `rows[0]` gives `x'`, `rows[1]` gives `y'`
    /// and `rows[2]` gives `w`, each as a dot product with `(x, y, 1)`.
    pub const fn new(rows: [[f64; 3]; 3]) -> Homography {
        Homography { rows }
    }

    /// Its rows, as [`Homography::new`] takes them.
    pub const fn rows(&self) -> [[f64; 3]; 3] {
        self.rows
    }

    /// The image of `p` before the division: `(x', y', w)`.
    pub fn map(&self, p: Point) -> [f64; 3] {
        let [a, b, c] = self.rows;
        let dot = |r: [f64; 3]| r[0] * p.x + r[1] * p.y + r[2];
        [dot(a), dot(b), dot(c)]
    }

    /// Where `p` is seen: `(x'/w, y'/w)`, or `None` when `w` is not
    /// positive, so the point is not in front.
    pub fn project(&self, p: Point) -> Option<Point> {
        let [x, y, w] = self.map(p);
        (w > 0.0).then(|| Point::new(x / w, y / w))
    }

    /// The determinant of the matrix.
    pub fn determinant(&self) -> f64 {
        let [[a, b, c], [d, e, f], [g, h, i]] = self.rows;
        a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
    }

    /// The map that undoes this one, or `None` when it is singular, as for
    /// a plane seen exactly edge-on.
    ///
    /// It is the true inverse, not one up to scale, so where `self` maps a
    /// point to depth `w`, the inverse maps what is seen back with
    /// `w' = 1/w`: positive in front, negative behind.
    pub fn inverse(&self) -> Option<Homography> {
        let [[a, b, c], [d, e, f], [g, h, i]] = self.rows;
        let det = self.determinant();
        // Singular relative to the largest the determinant could be for
        // rows of these lengths (Hadamard's bound), not relative to 1.
        let bound: f64 = self
            .rows
            .iter()
            .map(|r| r.iter().map(|v| v * v).sum::<f64>().sqrt())
            .product();
        if !det.is_finite() || det.abs() <= f64::EPSILON * bound {
            return None;
        }
        let adjugate = [
            [e * i - f * h, c * h - b * i, b * f - c * e],
            [f * g - d * i, a * i - c * g, c * d - a * f],
            [d * h - e * g, b * g - a * h, a * e - b * d],
        ];
        Some(Homography {
            rows: adjugate.map(|row| row.map(|v| v / det)),
        })
    }

    /// The Jacobian of the seen map at `p`, `[∂x/∂u, ∂x/∂v, ∂y/∂u, ∂y/∂v]`,
    /// or `None` where `p` is not in front.
    pub(crate) fn jacobian(&self, p: Point) -> Option<[f64; 4]> {
        let [x, y, w] = self.map(p);
        if w <= 0.0 {
            return None;
        }
        let (sx, sy) = (x / w, y / w);
        let [r0, r1, r2] = self.rows;
        Some([
            (r0[0] - sx * r2[0]) / w,
            (r0[1] - sx * r2[1]) / w,
            (r1[0] - sy * r2[0]) / w,
            (r1[1] - sy * r2[1]) / w,
        ])
    }

    /// The affine map that matches this one to first order at `p`: the
    /// same point and the same Jacobian there. `None` where `p` is not in
    /// front.
    pub(crate) fn tangent(&self, p: Point) -> Option<Affine> {
        let seen = self.project(p)?;
        let [a, c, b, d] = self.jacobian(p)?;
        let linear = Affine::new([a, b, c, d, 0.0, 0.0]);
        let moved = seen - linear * p;
        Some(Affine::translate(moved) * linear)
    }

    /// How many px of the image one unit at `p` spans along the direction
    /// it is stretched most: the larger singular value of the Jacobian.
    /// Zero where `p` is not in front.
    pub(crate) fn magnification(&self, p: Point) -> f64 {
        self.jacobian(p).map_or(0.0, |j| singular_values(j)[0])
    }
}

/// The singular values of the 2×2 matrix `[a, b, c, d]` (rows), largest
/// first.
pub(crate) fn singular_values([a, b, c, d]: [f64; 4]) -> [f64; 2] {
    let sum = a * a + b * b + c * c + d * d;
    let det = a * d - b * c;
    let root = (sum * sum - 4.0 * det * det).max(0.0).sqrt();
    [
        ((sum + root) / 2.0).sqrt(),
        ((sum - root) / 2.0).max(0.0).sqrt(),
    ]
}

impl Default for Homography {
    fn default() -> Homography {
        Homography::IDENTITY
    }
}

impl Mul for Homography {
    type Output = Homography;

    fn mul(self, other: Homography) -> Homography {
        let (a, b) = (self.rows, other.rows);
        Homography {
            rows: std::array::from_fn(|r| {
                std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum())
            }),
        }
    }
}

impl From<Affine> for Homography {
    fn from(affine: Affine) -> Homography {
        let [a, b, c, d, e, f] = affine.as_coeffs();
        Homography {
            rows: [[a, c, e], [b, d, f], [0.0, 0.0, 1.0]],
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/homography.rs"]
mod tests;
