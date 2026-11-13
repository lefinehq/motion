//! Samples easing curves: how progress through an animation maps to movement.
//!
//! Part of step 2 (Compile), used by step 3 (Evaluate). The curves
//! themselves are parsed by `motion_format`; this turns one into a value.

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

/// One coordinate of a cubic bézier from (0,0) to (1,1) with control values
/// `a` and `b`, at parameter `t`.
fn coord(a: f64, b: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
}

/// Solves the bézier with control points `(x1, y1)` and `(x2, y2)` for the
/// `y` that belongs to `x == p`.
fn bezier(x1: f64, y1: f64, x2: f64, y2: f64, p: f64) -> f64 {
    // The curve is given as x(t), y(t), but we need y for a known x, so we
    // first find t. x(t) is monotonic on [0, 1] whenever x1 and x2 lie in
    // 0..=1 (the format enforces this for custom curves), so
    // bisection always converges. We use it because it is robust and has no
    // special cases; speed does not matter yet. 40 halvings narrow the
    // interval to about 1e-12, far below a pixel.
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
