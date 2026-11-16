//! A property's value at any time, from its keys, in the project's own units:
//! what an expression reads as `value` and `valueAtTime(t)`.
//!
//! It reads the keys as the map writes them for the scene, so a baked
//! expression sees what the scene draws for the properties it reads:
//! - a hold keeps the key's value until the next key;
//! - two linear sides make a straight line;
//! - otherwise each side's ease is a speed in value units a second and an
//!   influence, the handles of a cubic in time, one per dimension where the
//!   project keeps one per dimension, else the first for every dimension;
//! - a key with spatial tangents moves along the bézier path they bend, at
//!   the speed its one ease gives along the path's length. A point with
//!   depth moves along its path in space and its length there, unless the
//!   span stays at one depth, where that is its flat path.
//!
//! Before the first key the value is the first key's, after the last the last's.
//! The eases and the paths are the scene's own ([`motion_scene::Ease`],
//! [`PathSpan`]), so the two cannot drift apart.

use kurbo::{Point, Vec2};
use motion_format::Curve;
use motion_scene::PathSpan;

use crate::model::{Color, Ease, Interpolation, Keyframe, Stream, Value};

impl Stream {
    /// The value at `time`, in composition seconds.
    pub fn value_at(&self, time: f64) -> Value {
        match self {
            Stream::Static(value) => value.clone(),
            Stream::Animated(keys) => keyed(keys, time),
        }
    }

    /// Whether the stream moves along a motion path: a key has spatial tangents.
    pub fn is_spatial(&self) -> bool {
        match self {
            Stream::Static(_) => false,
            Stream::Animated(keys) => keys.iter().any(|k| k.spatial.is_some()),
        }
    }
}

impl Value {
    /// The numbers of a number, vector or colour (`[r, g, b, a]`); none for
    /// a path, text or an opaque value.
    pub fn numbers(&self) -> Option<Vec<f64>> {
        Some(match self {
            Value::Number(n) => vec![*n],
            Value::Vector2(v) => v.to_vec(),
            Value::Vector3(v) => v.to_vec(),
            Value::Color(c) => vec![c.r, c.g, c.b, c.a],
            Value::Path(_)
            | Value::Text(_)
            | Value::Data(_)
            | Value::Layer(_)
            | Value::Marker(_)
            | Value::Mask(_)
            | Value::Opaque { .. } => {
                return None;
            }
        })
    }

    /// This value's kind with `numbers` in place of its own; components
    /// `numbers` lacks keep their value.
    pub fn with_numbers(&self, numbers: &[f64]) -> Value {
        let at = |i: usize, own: f64| numbers.get(i).copied().unwrap_or(own);
        match self {
            Value::Number(n) => Value::Number(at(0, *n)),
            Value::Vector2(v) => Value::Vector2([at(0, v[0]), at(1, v[1])]),
            Value::Vector3(v) => Value::Vector3([at(0, v[0]), at(1, v[1]), at(2, v[2])]),
            Value::Color(c) => Value::Color(Color {
                r: at(0, c.r),
                g: at(1, c.g),
                b: at(2, c.b),
                a: at(3, c.a),
            }),
            other => other.clone(),
        }
    }
}

/// The value of keys sorted by time at `time`. Of several keys at one time,
/// the last applies from then on.
fn keyed(keys: &[Keyframe], time: f64) -> Value {
    let after = keys.partition_point(|k| k.time <= time);
    let Some(i) = after.checked_sub(1) else {
        return keys.first().map_or(Value::Number(0.0), |k| k.value.clone());
    };
    let (key, Some(next)) = (&keys[i], keys.get(after)) else {
        return keys[i].value.clone();
    };
    if key.out_interpolation == Interpolation::Hold {
        return key.value.clone();
    }
    let seconds = next.time - key.time;
    let p = (time - key.time) / seconds;
    let (Some(from), Some(to)) = (key.value.numbers(), next.value.numbers()) else {
        // Paths and text step at each key.
        return key.value.clone();
    };
    if key.spatial.is_some() && from.len() >= 2 {
        let along_path = match depth(key, next) {
            Depth::Level => spatial(key, next, &from, &to, seconds, p),
            Depth::Straight | Depth::Bent => in_space(key, next, seconds, p),
        };
        return key.value.with_numbers(&along_path);
    }
    let numbers: Vec<f64> = from
        .iter()
        .zip(&to)
        .enumerate()
        .map(|(dim, (&a, &b))| {
            motion_scene::Ease::from_curve(curve(key, next, dim), b - a, seconds).sample(a, b, p)
        })
        .collect();
    key.value.with_numbers(&numbers)
}

/// The point at progress `p` along the flat motion path from `key` to
/// `next`; a third dimension, which stays put, moves in step with the
/// distance along it.
fn spatial(
    key: &Keyframe,
    next: &Keyframe,
    from: &[f64],
    to: &[f64],
    seconds: f64,
    p: f64,
) -> Vec<f64> {
    let handle = |t: Option<&Vec<f64>>| {
        let t = t.map(Vec::as_slice).unwrap_or_default();
        Vec2::new(
            t.first().copied().unwrap_or(0.0),
            t.get(1).copied().unwrap_or(0.0),
        )
    };
    let span = PathSpan::new(
        Point::new(from[0], from[1]),
        Point::new(to[0], to[1]),
        [
            handle(key.spatial.as_ref().map(|s| &s.out_tangent)),
            handle(next.spatial.as_ref().map(|s| &s.in_tangent)),
        ],
        curve(key, next, 0),
        seconds,
    );
    let (point, covered) = span.sample(p);
    let mut out = vec![point.x, point.y];
    out.extend(
        from.iter()
            .zip(to)
            .skip(2)
            .map(|(a, b)| a + (b - a) * covered),
    );
    out
}

/// How the span of a point from `key` to `next` moves in depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    /// It stays at one depth, or has none: its path is flat.
    Level,
    /// It moves along a straight line in space, one way: its handles, if
    /// any, lie on the line between its ends.
    Straight,
    /// It moves along a curve in space that changes depth: a path its
    /// flat projection and depth alone cannot time.
    Bent,
}

/// How the span from `key` to `next` moves in depth. A span is level when
/// its two ends and both handles have no depth to change. It is straight
/// when each handle lies on the line from one end to the other, within
/// [`ON_LINE`], as automatic handles of a move in a line do: the path then
/// covers the line once, from one end to the other, and timed along its
/// length it is the line's own motion.
pub(crate) fn depth(key: &Keyframe, next: &Keyframe) -> Depth {
    let numbers = |k: &Keyframe| k.value.numbers().unwrap_or_default();
    let (from, to) = (numbers(key), numbers(next));
    let handles = [
        key.spatial.as_ref().map(|s| s.out_tangent.as_slice()),
        next.spatial.as_ref().map(|s| s.in_tangent.as_slice()),
    ];
    let handle = |dim: usize| handles.iter().any(|h| !negligible(component(*h, dim)));
    let level = match (from.get(2), to.get(2)) {
        (Some(a), Some(b)) => negligible(b - a) && !handle(2),
        _ => true,
    };
    if level {
        return Depth::Level;
    }
    let at = |n: &[f64]| [0, 1, 2].map(|d| n.get(d).copied().unwrap_or(0.0));
    let (from, to) = (at(&from), at(&to));
    let chord = sub(to, from);
    let length = norm(chord);
    // How far along the line from `from` a handle's point lies, if on it.
    let along = |point: [f64; 3]| {
        let offset = sub(point, from);
        let s = dot(offset, chord) / length;
        let off_line = norm(sub(offset, chord.map(|c| c * s / length)));
        (off_line <= ON_LINE && (-ON_LINE..=length + ON_LINE).contains(&s)).then_some(s)
    };
    let [out, into] = handles.map(|h| [0, 1, 2].map(|d| component(h, d)));
    let on_line = length > 0.0 && along(add(from, out)).is_some() && along(add(to, into)).is_some();
    match on_line || !(0..3).any(handle) {
        true => Depth::Straight,
        false => Depth::Bent,
    }
}

/// How far from the line between its ends, in px, a handle may lie for a
/// span to count as straight: far below what a frame shows.
const ON_LINE: f64 = 1e-3;

/// `a` plus `b`.
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The dot product of `a` and `b`.
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Whether a coordinate is zero at the precision a scene writes.
fn negligible(n: f64) -> bool {
    n.abs() < 5e-7
}

/// Component `dim` of a handle, 0 where it has none.
fn component(handle: Option<&[f64]>, dim: usize) -> f64 {
    handle.and_then(|h| h.get(dim)).copied().unwrap_or(0.0)
}

/// The point with depth at progress `p` along the motion path in space
/// from `key` to `next`, timed by its one ease along the path's length.
fn in_space(key: &Keyframe, next: &Keyframe, seconds: f64, p: f64) -> Vec<f64> {
    let point = |k: &Keyframe| {
        let n = k.value.numbers().unwrap_or_default();
        [0, 1, 2].map(|dim| n.get(dim).copied().unwrap_or(0.0))
    };
    let handle = |h: Option<&Vec<f64>>| [0, 1, 2].map(|dim| component(h.map(Vec::as_slice), dim));
    let path = Path3::new(
        point(key),
        point(next),
        [
            handle(key.spatial.as_ref().map(|s| &s.out_tangent)),
            handle(next.spatial.as_ref().map(|s| &s.in_tangent)),
        ],
    );
    let ease = motion_scene::Ease::from_curve(curve(key, next, 0), path.length, seconds);
    path.at_length(ease.sample(0.0, path.length, p)).to_vec()
}

/// A cubic bézier in space, measured along its length: the motion path of
/// a point with depth. The length is integrated by Gauss-Legendre
/// quadrature over equal pieces of the curve's parameter.
#[derive(Debug, Clone)]
pub(crate) struct Path3 {
    /// The ends and the control points between them.
    points: [[f64; 3]; 4],
    /// The length up to the end of each piece.
    lengths: [f64; PIECES],
    /// The whole length.
    pub length: f64,
}

/// How many pieces the length is integrated over.
const PIECES: usize = 32;

/// Five-point Gauss-Legendre nodes on `[-1, 1]` and their weights.
const GAUSS: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
    (0.906_179_845_938_664, 0.236_926_885_056_189_1),
];

impl Path3 {
    /// The path from `from` to `to`, leaving along the handle `out` and
    /// arriving along `into`, each an offset from its own end.
    pub(crate) fn new(from: [f64; 3], to: [f64; 3], [out, into]: [[f64; 3]; 2]) -> Path3 {
        let mut path = Path3 {
            points: [from, add(from, out), add(to, into), to],
            lengths: [0.0; PIECES],
            length: 0.0,
        };
        let mut total = 0.0;
        for i in 0..PIECES {
            total += path.measure(i as f64 / PIECES as f64, (i + 1) as f64 / PIECES as f64);
            path.lengths[i] = total;
        }
        path.length = total;
        path
    }

    /// The point at parameter `t`.
    pub(crate) fn point(&self, t: f64) -> [f64; 3] {
        let [p0, p1, p2, p3] = self.points;
        let u = 1.0 - t;
        let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        [0, 1, 2].map(|d| {
            weights[0] * p0[d] + weights[1] * p1[d] + weights[2] * p2[d] + weights[3] * p3[d]
        })
    }

    /// The derivative at parameter `t`.
    fn derivative(&self, t: f64) -> [f64; 3] {
        let [p0, p1, p2, p3] = self.points;
        let u = 1.0 - t;
        [0, 1, 2].map(|d| {
            3.0 * u * u * (p1[d] - p0[d])
                + 6.0 * u * t * (p2[d] - p1[d])
                + 3.0 * t * t * (p3[d] - p2[d])
        })
    }

    /// The length between parameters `a` and `b`, within one piece.
    fn measure(&self, a: f64, b: f64) -> f64 {
        let half = (b - a) / 2.0;
        GAUSS
            .iter()
            .map(|(x, w)| w * norm(self.derivative(a + half * (1.0 + x))))
            .sum::<f64>()
            * half
    }

    /// The point `along` px from the start. Beyond either end, as an
    /// overshooting ease reaches, the path carries on straight.
    pub(crate) fn at_length(&self, along: f64) -> [f64; 3] {
        let [p0, p1, p2, p3] = self.points;
        if self.length == 0.0 {
            return p0;
        }
        if along <= 0.0 || along >= self.length {
            let (end, candidates, beyond) = match along <= 0.0 {
                true => (p0, [sub(p0, p1), sub(p0, p2), sub(p0, p3)], -along),
                false => (
                    p3,
                    [sub(p3, p2), sub(p3, p1), sub(p3, p0)],
                    along - self.length,
                ),
            };
            let direction = candidates.into_iter().find(|v| norm(*v) > 0.0);
            return match direction {
                Some(v) => [0, 1, 2].map(|d| end[d] + v[d] * beyond / norm(v)),
                None => end,
            };
        }
        let piece = self.lengths.partition_point(|&l| l < along).min(PIECES - 1);
        let start = piece.checked_sub(1).map_or(0.0, |i| self.lengths[i]);
        let (mut lo, mut hi) = (
            piece as f64 / PIECES as f64,
            (piece + 1) as f64 / PIECES as f64,
        );
        let from = lo;
        // The length grows with the parameter, so halving the bracket
        // finds where it reaches `along`, to well below a thousandth of a px.
        for _ in 0..48 {
            let mid = f64::midpoint(lo, hi);
            match start + self.measure(from, mid) < along {
                true => lo = mid,
                false => hi = mid,
            }
        }
        self.point(f64::midpoint(lo, hi))
    }
}

/// `a` less `b`.
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The length of `v`.
fn norm(v: [f64; 3]) -> f64 {
    v.iter().map(|c| c * c).sum::<f64>().sqrt()
}

/// The curve of dimension `dim` from `key` to `next`, in the project's units
/// a second: `hold`, `linear`, or the eases on both sides as speeds, the
/// first ease for every dimension where the project keeps one. A linear side
/// reaches nowhere into the span. The map writes the same curve, so a baked
/// expression reads what the scene draws.
pub(crate) fn curve(key: &Keyframe, next: &Keyframe, dim: usize) -> Curve {
    let linear = |i: Interpolation| i == Interpolation::Linear;
    if key.out_interpolation == Interpolation::Hold {
        return Curve::Hold;
    }
    if linear(key.out_interpolation) && linear(next.in_interpolation) {
        return Curve::Linear;
    }
    let side = |eases: &[Ease], interpolation: Interpolation| match (
        linear(interpolation),
        eases.get(dim).or_else(|| eases.first()),
    ) {
        (false, Some(e)) => (e.speed, e.influence.clamp(0.0, 1.0)),
        _ => (0.0, 0.0),
    };
    let (out_speed, out_reach) = side(&key.out_ease, key.out_interpolation);
    let (in_speed, in_reach) = side(&next.in_ease, next.in_interpolation);
    Curve::Speed {
        out_speed,
        out_reach,
        in_speed,
        in_reach,
    }
}

#[cfg(test)]
#[path = "../tests/unit/eval.rs"]
mod tests;
