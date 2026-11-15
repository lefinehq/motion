//! A spring ease: a damped oscillator released from rest at 0, settling at
//! rest on 1, set by how much it bounces and how long it is perceived to
//! take. Its shape depends on the bounce alone; the perceived duration only
//! scales its time, and a span of known length scales it to fit.
//!
//! The settle time is found exactly, not by stepping: the residual of an
//! underdamped spring peaks at known times with known heights, so the last
//! crossing of the rest threshold lies between the last peak above it and
//! the next zero, where the residual is monotonic; a critically damped or
//! overdamped spring's residual only falls.

use std::f64::consts::{PI, TAU};

use crate::units::Time;

/// How close to the end a spring must stay to be at rest: a fraction of the
/// change, half a pixel on a move of a thousand.
pub const REST: f64 = 5e-4;

/// The perceived duration of a spring that does not give one.
pub const PERCEIVED: Time = Time::Seconds(0.5);

/// A spring ease.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// How much it bounces, in (-1, 1): 0 settles without overshoot as fast
    /// as it can, more bounces more, less creeps in slower.
    pub bounce: f64,
    /// How long the move is perceived to take, if given; [`PERCEIVED`] by
    /// default. A behaviour without a duration lasts the spring's settle time
    /// at this perceived duration.
    pub perceived: Option<Time>,
}

impl Spring {
    /// The damping ratio: `1 - bounce` for a bounce of 0 or more, else
    /// `1 / (1 + bounce)`.
    pub fn damping_ratio(self) -> f64 {
        match self.bounce >= 0.0 {
            true => 1.0 - self.bounce,
            false => 1.0 / (1.0 + self.bounce),
        }
    }

    /// The perceived duration, given or [`PERCEIVED`].
    pub fn perceived(self) -> Time {
        self.perceived.unwrap_or(PERCEIVED)
    }

    /// How long the spring takes to come to rest, in the perceived
    /// duration's unit: the perceived duration is one period of the
    /// undamped spring.
    pub fn settle(self) -> Time {
        let scale = Motion::new(self.damping_ratio()).settle / TAU;
        match self.perceived() {
            Time::Frames(f) => Time::Frames(f * scale),
            Time::Seconds(s) => Time::Seconds(s * scale),
        }
    }

    /// The motion this spring makes, for evaluating it many times.
    pub fn motion(self) -> Motion {
        Motion::new(self.damping_ratio())
    }
}

/// A spring's motion at an undamped angular frequency of 1, with its settle
/// time solved once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    zeta: f64,
    /// The time it comes to rest, in radians of the undamped spring.
    settle: f64,
}

impl Motion {
    fn new(zeta: f64) -> Motion {
        let mut motion = Motion { zeta, settle: 0.0 };
        motion.settle = motion.solve_settle();
        motion
    }

    /// Progress at `p` in 0..=1 of a span that lasts the settle time: 0 at 0
    /// and exactly 1 at 1. What is left of the motion at the settle time
    /// (under [`REST`]) is taken out evenly over the span, so the end meets.
    pub fn progress(self, p: f64) -> f64 {
        let t = p * self.settle;
        1.0 - self.residual(t) + p * self.residual(self.settle)
    }

    /// `1 - x(t)`: how far from the end the spring is at time `t`, released
    /// from rest.
    fn residual(self, t: f64) -> f64 {
        let zeta = self.zeta;
        if zeta < 1.0 {
            let wd = libm::sqrt(1.0 - zeta * zeta);
            let b = zeta / wd;
            libm::exp(-zeta * t) * (libm::cos(wd * t) + b * libm::sin(wd * t))
        } else if zeta == 1.0 {
            (1.0 + t) * libm::exp(-t)
        } else {
            let wd = libm::sqrt(zeta * zeta - 1.0);
            let b = zeta / wd;
            ((1.0 + b) * libm::exp((-zeta + wd) * t) + (1.0 - b) * libm::exp((-zeta - wd) * t))
                / 2.0
        }
    }

    /// The first time after which the residual stays under [`REST`].
    fn solve_settle(self) -> f64 {
        let zeta = self.zeta;
        let below = |t: f64| self.residual(t).abs() < REST;
        if zeta < 1.0 {
            // Extremes of the residual fall at k·π/ωd with heights
            // e^(-ζkπ/ωd); the last at or above the threshold is peak k.
            let wd = libm::sqrt(1.0 - zeta * zeta);
            let k = libm::floor(libm::log(1.0 / REST) * wd / (zeta * PI));
            let peak = k * PI / wd;
            // The next zero: cos(ωd t) + b sin(ωd t) = 0 after the peak.
            let zero = (k * PI + PI - libm::atan(wd / zeta)) / wd;
            return bisect(peak, zero, below);
        }
        let mut hi = 1.0;
        while !below(hi) {
            hi *= 2.0;
        }
        bisect(0.0, hi, below)
    }
}

/// The time in `lo..=hi` where `below` turns true, to well under a frame of
/// any spring a scene can hold: `below(lo)` is false and `below(hi)` true.
fn bisect(mut lo: f64, mut hi: f64, below: impl Fn(f64) -> bool) -> f64 {
    for _ in 0..64 {
        let mid = (lo + hi) / 2.0;
        match below(mid) {
            true => hi = mid,
            false => lo = mid,
        }
    }
    hi
}

#[cfg(test)]
#[path = "../tests/unit/spring.rs"]
mod tests;
