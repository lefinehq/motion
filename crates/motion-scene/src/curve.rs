//! Samples easing curves: maps progress through an animation to eased progress.

use motion_format::Curve;

/// Maps linear progress `p` (clamped to 0..=1) to eased progress.
pub fn sample(curve: Curve, p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    // Pin the endpoints so bisection round-off never leaks out as 1e-25.
    if p == 0.0 || p == 1.0 {
        return p;
    }
    match curve {
        Curve::Linear => p,
        // p is below 1 here: the endpoints returned above.
        Curve::Hold => 0.0,
        Curve::Bezier(x1, y1, x2, y2) => bezier(x1, y1, x2, y2, p),
    }
}

/// One coordinate at `t` of a cubic bézier from 0 to 1 with control values `a` and `b`.
fn coord(a: f64, b: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
}

/// The `y` of the bézier with controls `(x1, y1)`, `(x2, y2)` where its `x` is `p`.
fn bezier(x1: f64, y1: f64, x2: f64, y2: f64, p: f64) -> f64 {
    // Bisect for t: robust, and x(t) is monotonic because the format keeps x1
    // and x2 in 0..=1. 40 halvings reach about 1e-12.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if coord(x1, x2, mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    coord(y1, y2, (lo + hi) / 2.0)
}

#[cfg(test)]
#[path = "../tests/unit/curve.rs"]
mod tests;
