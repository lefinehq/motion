//! One-dimensional easing in value space ([D31]): a span from `v0` to `v1`
//! eased by a cubic bézier whose handles reach into the span by a fraction of
//! its time and away from the ends by an amount of value, or by a family's
//! curve or steps, evaluated exactly at every frame.
//!
//! [D31]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d31-typed-values-and-value-space-keys

use motion_format::{Curve, Family, Shape};

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
    /// `v0 + (v1 - v0)·g(p)` for a family's curve `g` run in a shape.
    Shaped(Shape, Family),
    /// `count` equal steps, jumping at the end of each, or at the start with
    /// `start`.
    Steps {
        /// How many steps.
        count: u32,
        /// Whether each step jumps at its start.
        start: bool,
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
            Curve::Shaped(shape, family) => Ease::Shaped(shape, family),
            Curve::Steps { count, start } => Ease::Steps { count, start },
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
            Ease::Shaped(shape, family) => v0 + (v1 - v0) * shaped(shape, family, p),
            Ease::Steps { count, start } => v0 + (v1 - v0) * steps(count, start, p),
        }
    }

    /// Whether the span holds its first value when both keys are equal: a
    /// bézier whose handles add no value, or any ease that scales the change.
    pub fn is_still(self) -> bool {
        match self {
            Ease::Bezier { out, into } => out.1 == 0.0 && into.1 == 0.0,
            Ease::Linear | Ease::Hold | Ease::Shaped(..) | Ease::Steps { .. } => true,
        }
    }
}

/// A family's "in" curve run in `shape`, at progress `t` in 0..=1.
pub fn shaped(shape: Shape, family: Family, t: f64) -> f64 {
    let f = |t: f64| eased_in(family, t);
    match shape {
        Shape::In => f(t),
        Shape::Out => 1.0 - f(1.0 - t),
        Shape::InOut if t < 0.5 => f(2.0 * t) / 2.0,
        Shape::InOut => 1.0 - f(2.0 - 2.0 * t) / 2.0,
        Shape::OutIn if t < 0.5 => (1.0 - f(1.0 - 2.0 * t)) / 2.0,
        Shape::OutIn => (f(2.0 * t - 1.0) + 1.0) / 2.0,
    }
}

/// The "in" curve of `family` at `t` in 0..=1: 0 at 0 and 1 at 1. The
/// transcendental functions come from `libm`, so every platform agrees.
fn eased_in(family: Family, t: f64) -> f64 {
    use std::f64::consts::{FRAC_PI_2, TAU};
    match family {
        Family::Power(p) => libm::pow(t, p),
        Family::Sine => 1.0 - libm::cos(t * FRAC_PI_2),
        Family::Circ => 1.0 - libm::sqrt((1.0 - t * t).max(0.0)),
        Family::Expo => expo(t),
        Family::Back(o) => (o + 1.0) * t * t * t - o * t * t,
        Family::Elastic { amplitude, period } => {
            let s = period / TAU * libm::asin(1.0 / amplitude);
            -amplitude * expo(t) * libm::sin(((1.0 - t) - s) * TAU / period)
        }
        Family::Bounce => 1.0 - bounce_out(1.0 - t),
    }
}

/// `(2^(10t) - 1) / 1023`: the exponential `2^(10t - 10)` moved and scaled
/// so it starts at exactly 0 and still ends at 1.
fn expo(t: f64) -> f64 {
    (libm::exp2(10.0 * t) - 1.0) / 1023.0
}

/// The classic bounce ending at 1: four parabolas of 7.5625 at 1/2.75 steps.
fn bounce_out(t: f64) -> f64 {
    const K: f64 = 7.5625;
    const D: f64 = 2.75;
    if t < 1.0 / D {
        K * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        K * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        K * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        K * t * t + 0.984_375
    }
}

/// `count` steps at `t`: `floor(t·n)/n`, or `ceil` with `start`.
fn steps(count: u32, start: bool, t: f64) -> f64 {
    let n = f64::from(count.max(1));
    match start {
        true => (t * n).ceil() / n,
        false => (t * n).floor() / n,
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
