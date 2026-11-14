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
//!   the speed its one ease gives along the path's length.
//!
//! Before the first key the value is the first key's, after the last the last's.

use kurbo::{CubicBez, ParamCurve, ParamCurveArclen, Point, Vec2};

use crate::model::{Color, Ease, Interpolation, Keyframe, Stream, Value};

/// How close an arc length must come, in pixels, as the scene measures it.
const ARC_ACCURACY: f64 = 1e-6;

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
            Value::Path(_) | Value::Text(_) | Value::Opaque { .. } => return None,
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
        let along_path = spatial(key, next, &from, &to, seconds, p);
        return key.value.with_numbers(&along_path);
    }
    let numbers: Vec<f64> = from
        .iter()
        .zip(&to)
        .enumerate()
        .map(|(dim, (&a, &b))| ease(key, next, dim, seconds).sample(a, b, p))
        .collect();
    key.value.with_numbers(&numbers)
}

/// The point at progress `p` along the motion path from `key` to `next`; a
/// third dimension moves in step with the distance along the path.
fn spatial(
    key: &Keyframe,
    next: &Keyframe,
    from: &[f64],
    to: &[f64],
    seconds: f64,
    p: f64,
) -> Vec<f64> {
    let tangent = |t: Option<&Vec<f64>>| {
        let t = t.map(Vec::as_slice).unwrap_or_default();
        Vec2::new(
            t.first().copied().unwrap_or(0.0),
            t.get(1).copied().unwrap_or(0.0),
        )
    };
    let (p0, p3) = (Point::new(from[0], from[1]), Point::new(to[0], to[1]));
    let path = CubicBez::new(
        p0,
        p0 + tangent(key.spatial.as_ref().map(|s| &s.out_tangent)),
        p3 + tangent(next.spatial.as_ref().map(|s| &s.in_tangent)),
        p3,
    );
    let length = path.arclen(ARC_ACCURACY);
    let along = ease(key, next, 0, seconds).sample(0.0, length, p);
    let point = match length {
        0.0 => path.p0,
        l if along <= 0.0 || along >= l => extend(&path, along, l),
        _ => path.eval(path.inv_arclen(along, ARC_ACCURACY)),
    };
    let fraction = if length > 0.0 { along / length } else { p };
    let mut out = vec![point.x, point.y];
    out.extend(
        from.iter()
            .zip(to)
            .skip(2)
            .map(|(a, b)| a + (b - a) * fraction),
    );
    out
}

/// The ease of dimension `dim` from `key` to `next`, `seconds` apart.
fn ease(key: &Keyframe, next: &Keyframe, dim: usize, seconds: f64) -> Bezier {
    let linear = |i: Interpolation| i == Interpolation::Linear;
    if linear(key.out_interpolation) && linear(next.in_interpolation) {
        return Bezier::Linear;
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
    Bezier::Handles {
        out: (out_reach, out_speed * out_reach * seconds),
        into: (in_reach, in_speed * in_reach * seconds),
    }
}

/// One dimension's span: straight, or a cubic in time whose handles reach
/// a fraction of the span in time and rise in value units.
#[derive(Debug, Clone, Copy)]
enum Bezier {
    Linear,
    Handles { out: (f64, f64), into: (f64, f64) },
}

impl Bezier {
    /// The value at progress `p` (clamped) from `v0` to `v1`.
    fn sample(self, v0: f64, v1: f64, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        if p == 0.0 {
            return v0;
        }
        if p == 1.0 {
            return v1;
        }
        match self {
            Bezier::Linear => v0 + (v1 - v0) * p,
            Bezier::Handles { out, into } => {
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

/// The `t` where a bézier in time from 0 to 1, with control times `x1` and
/// `x2` in 0..=1, reaches `p`.
fn solve(x1: f64, x2: f64, p: f64) -> f64 {
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

/// A point `along` from the start of `path`, before its start or past its
/// end: the end tangent carried on in a straight line.
fn extend(path: &CubicBez, along: f64, length: f64) -> Point {
    let (end, toward, beyond) = if along <= 0.0 {
        (path.p0, path.p0 - tangent_at(path, true), -along)
    } else {
        (path.p3, path.p3 + tangent_at(path, false), along - length)
    };
    let direction = toward - end;
    match direction.hypot() {
        0.0 => end,
        n => end + direction * (beyond / n),
    }
}

/// The direction of travel at the start or the end; with a zero handle,
/// toward the next control point that differs.
fn tangent_at(path: &CubicBez, start: bool) -> Vec2 {
    let candidates = if start {
        [path.p1 - path.p0, path.p2 - path.p0, path.p3 - path.p0]
    } else {
        [path.p3 - path.p2, path.p3 - path.p1, path.p3 - path.p0]
    };
    candidates
        .into_iter()
        .find(|v| v.hypot() > 0.0)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../tests/unit/eval.rs"]
mod tests;
