//! One-dimensional easing in value space ([D31]): a span from `v0` to `v1`
//! eased by a cubic bézier whose handles reach into the span by a fraction of
//! its time and away from the ends by an amount of value.
//!
//! [D31]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d31-typed-values-and-value-space-keys

use motion_format::Curve;

/// How one number moves across a span between two keys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ease {
    /// Constant speed.
    Linear,
    /// The first value until the span ends.
    Hold,
    /// A cubic bézier through `(0, v0)`, `(out.0, v0 + out.1)`,
    /// `(1 - into.0, v1 - into.1)` and `(1, v1)`: each handle is how far it
    /// reaches into the span (0 to 1) and how much value it adds on the way.
    Bezier {
        /// The handle leaving the first key: `(reach, rise)`.
        out: (f64, f64),
        /// The handle arriving at the second key: `(reach, rise)`.
        into: (f64, f64),
    },
}

impl Ease {
    /// The ease `curve` gives a span that changes by `delta` over `seconds`.
    pub fn from_curve(curve: Curve, delta: f64, seconds: f64) -> Ease {
        match curve {
            Curve::Linear => Ease::Linear,
            Curve::Hold => Ease::Hold,
            // A normalised curve scales with the span's change.
            Curve::Bezier(x1, y1, x2, y2) => Ease::Bezier {
                out: (x1, y1 * delta),
                into: (1.0 - x2, (1.0 - y2) * delta),
            },
            // A speed is value per second; the handle's rise is speed times its length in time.
            Curve::Speed {
                out_speed,
                out_reach,
                in_speed,
                in_reach,
            } => Ease::Bezier {
                out: (out_reach, out_speed * out_reach * seconds),
                into: (in_reach, in_speed * in_reach * seconds),
            },
        }
    }

    /// The value at progress `p` (clamped to 0..=1) through a span from `v0` to `v1`.
    pub fn sample(self, v0: f64, v1: f64, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        // Pin the ends so bisection round-off never leaks out.
        if p == 0.0 {
            return v0;
        }
        if p == 1.0 {
            return v1;
        }
        match self {
            Ease::Linear => v0 + (v1 - v0) * p,
            Ease::Hold => v0,
            Ease::Bezier { out, into } => {
                let t = solve(out.0, 1.0 - into.0, p);
                cubic(v0, v0 + out.1, v1 - into.1, v1, t)
            }
        }
    }
}

/// One coordinate at `t` of a cubic bézier with control values `p0`..`p3`.
fn cubic(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

/// The `t` where the time coordinate of a bézier from 0 to 1 with control
/// times `x1` and `x2` (each in 0..=1, so it never runs backwards) is `p`.
fn solve(x1: f64, x2: f64, p: f64) -> f64 {
    // Bisection is robust on a monotonic curve; 48 halvings reach about 4e-15.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if cubic(0.0, x1, x2, 1.0, mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

#[cfg(test)]
#[path = "../tests/unit/curve.rs"]
mod tests;
