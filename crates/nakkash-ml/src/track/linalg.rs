//! The small linear algebra the fits need: the eigenvectors of a symmetric
//! matrix, by Jacobi rotations, and 3×3 products.

/// A 3×3 matrix, row by row.
pub type Mat3 = [[f64; 3]; 3];

/// The identity.
pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// The product `a · b`.
pub fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            *value = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

/// `point` through the projective map `h`, or `None` where `h` sends it to
/// infinity.
pub fn apply(h: &Mat3, [x, y]: [f64; 2]) -> Option<[f64; 2]> {
    let w = h[2][0] * x + h[2][1] * y + h[2][2];
    if w.abs() < 1e-12 || !w.is_finite() {
        return None;
    }
    Some([
        (h[0][0] * x + h[0][1] * y + h[0][2]) / w,
        (h[1][0] * x + h[1][1] * y + h[1][2]) / w,
    ])
}

/// The inverse of `m`, or `None` when it is singular.
pub fn inverse(m: &Mat3) -> Option<Mat3> {
    let c =
        |r0: usize, c0: usize, r1: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let det = m[0][0] * c(1, 1, 2, 2) - m[0][1] * c(1, 0, 2, 2) + m[0][2] * c(1, 0, 2, 1);
    if det.abs() < 1e-15 || !det.is_finite() {
        return None;
    }
    let adj = [
        [c(1, 1, 2, 2), -c(0, 1, 2, 2), c(0, 1, 1, 2)],
        [-c(1, 0, 2, 2), c(0, 0, 2, 2), -c(0, 0, 1, 2)],
        [c(1, 0, 2, 1), -c(0, 0, 2, 1), c(0, 0, 1, 1)],
    ];
    Some(adj.map(|row| row.map(|v| v / det)))
}

/// The eigenvalues and eigenvectors of the symmetric `n`×`n` matrix `a`
/// (row by row), by cyclic Jacobi rotations until the off-diagonal part is
/// negligible: the values in increasing order, each vector a column of the
/// second result, `vectors[k * n + i]` being component `k` of vector `i`.
pub fn symmetric_eigen(a: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    debug_assert_eq!(a.len(), n * n);
    let mut a = a.to_vec();
    let mut v = vec![0.0; n * n];
    (0..n).for_each(|i| v[i * n + i] = 1.0);
    let norm: f64 = a
        .iter()
        .map(|x| x * x)
        .sum::<f64>()
        .sqrt()
        .max(f64::MIN_POSITIVE);
    for _sweep in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i * n + j] * a[i * n + j])
            .sum::<f64>()
            .sqrt();
        if off <= 1e-15 * norm {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                if apq.abs() <= f64::MIN_POSITIVE {
                    continue;
                }
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let sign = if theta >= 0.0 { 1.0 } else { -1.0 };
                let t = sign / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k * n + p], a[k * n + q]);
                    a[k * n + p] = c * akp - s * akq;
                    a[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p * n + k], a[q * n + k]);
                    a[p * n + k] = c * apk - s * aqk;
                    a[q * n + k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let (vkp, vkq) = (v[k * n + p], v[k * n + q]);
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| a[i * n + i].total_cmp(&a[j * n + j]));
    let values = order.iter().map(|&i| a[i * n + i]).collect();
    let mut vectors = vec![0.0; n * n];
    for (to, &from) in order.iter().enumerate() {
        for k in 0..n {
            vectors[k * n + to] = v[k * n + from];
        }
    }
    (values, vectors)
}

#[cfg(test)]
#[path = "../../tests/unit/track/linalg.rs"]
mod tests;
