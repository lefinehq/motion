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
            Value::Path(_) | Value::Text(_) | Value::Data(_) | Value::Opaque { .. } => {
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
        let along_path = spatial(key, next, &from, &to, seconds, p);
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
