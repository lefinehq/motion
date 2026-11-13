//! Easing curves: how progress through an animation maps to movement.
//!
//! Part of step 2 (Compile), used by step 3 (Evaluate). Curves are cubic
//! béziers, the same model used by CSS timing functions.

use motion_format::shorthand::Easing;

/// Maps linear progress `p` (clamped to 0..=1) to eased progress.
pub fn sample(e: Easing, p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    // Pin the endpoints so bisection round-off never leaks out as 1e-25.
    if p == 0.0 || p == 1.0 {
        return p;
    }
    match e {
        Easing::Linear => p,
        Easing::EaseIn => bezier(0.42, 0.0, 1.0, 1.0, p),
        Easing::EaseOut => bezier(0.0, 0.0, 0.58, 1.0, p),
        Easing::EaseInOut => bezier(0.42, 0.0, 0.58, 1.0, p),
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
    // first find t. x(t) is monotonic on [0, 1] for these control points, so
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
mod tests {
    use super::*;

    #[test]
    fn easings_hit_endpoints_and_known_midpoints() {
        for e in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert!(sample(e, 0.0).abs() < 1e-9 && (sample(e, 1.0) - 1.0).abs() < 1e-9);
        }
        assert_eq!(sample(Easing::Linear, 0.25), 0.25);
        assert!((sample(Easing::EaseOut, 0.5) - 0.684).abs() < 1e-3);
        assert!((sample(Easing::EaseInOut, 0.5) - 0.5).abs() < 1e-6);
        assert!(
            (0..=100)
                .map(|i| sample(Easing::EaseIn, i as f64 / 100.0))
                .collect::<Vec<_>>()
                .windows(2)
                .all(|w| w[1] >= w[0])
        );
        assert_eq!(sample(Easing::EaseOut, -1.0), 0.0); // clamps
    }

    #[test]
    fn clamps_above_one() {
        assert_eq!(sample(Easing::EaseIn, 2.0), 1.0);
    }

    #[test]
    fn ease_in_is_slow_at_first_and_ease_out_fast() {
        assert!(sample(Easing::EaseIn, 0.25) < 0.25);
        assert!(sample(Easing::EaseOut, 0.25) > 0.25);
    }
}
