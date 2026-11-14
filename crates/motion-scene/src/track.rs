//! Keyframe tracks: one property's typed value over a layer's own time, in
//! frames. Each span between two keys eases each component in value space
//! ([`Ease`]); a spatial span instead moves along a bézier path at a speed its
//! one ease gives along the path's length.

use kurbo::{CubicBez, ParamCurve, ParamCurveArclen, Point};
use motion_format::anim::Easing;
use motion_format::{Value, ValueType};

use crate::curve::Ease;

/// How close an arc length must come, in pixels.
const ARC_ACCURACY: f64 = 1e-6;

/// One key as the compiler hands it over: a value at a frame, how the span
/// after it eases, and on a spatial track how the path bends.
#[derive(Debug, Clone, PartialEq)]
pub struct KeySpec {
    /// The frame the key sits on.
    pub frame: f64,
    /// The value there.
    pub value: Value,
    /// How the span to the next key eases.
    pub easing: Easing,
    /// The bend of the path to the next key: the leaving handle as an offset
    /// from this value, the arriving one from the next key's value.
    pub path: Option<[[f64; 2]; 2]>,
}

impl KeySpec {
    /// A key with a linear span after it and a straight path.
    pub fn linear(frame: f64, value: Value) -> KeySpec {
        KeySpec {
            frame,
            value,
            easing: Easing::All(motion_format::Curve::Linear),
            path: None,
        }
    }
}

/// One key of a compiled track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// The frame the key sits on.
    pub frame: f64,
    /// The value there.
    pub value: Value,
}

/// The span from one key to the next.
#[derive(Debug, Clone, PartialEq)]
enum Segment {
    /// Each component eases on its own.
    Components(Vec<Ease>),
    /// The value moves along `path`, whose length is `length`, eased along that length.
    Path {
        path: CubicBez,
        length: f64,
        ease: Ease,
    },
}

/// The keys of one value: never empty, sorted by frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    ty: ValueType,
    keys: Vec<Key>,
    /// One per pair of neighbouring keys.
    segments: Vec<Segment>,
}

impl Track {
    /// Builds a track of `ty` values from `keys`, sorted stably by frame, at
    /// `fps` frames per second (speeds are per second). With `spatial`, every
    /// span of a point moves along a path: straight, or bent by the key's `path`.
    ///
    /// # Panics
    /// If `keys` is empty or a key's value is not of type `ty`.
    pub fn new(ty: ValueType, mut keys: Vec<KeySpec>, fps: f64, spatial: bool) -> Track {
        assert!(!keys.is_empty(), "a track needs at least one key");
        assert!(
            keys.iter().all(|k| k.value.value_type() == ty),
            "keys of one type"
        );
        keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        let spatial = spatial && ty == ValueType::Point;
        let segments = keys
            .windows(2)
            .map(|pair| segment(&pair[0], &pair[1], fps, spatial))
            .collect();
        Track {
            ty,
            keys: keys
                .into_iter()
                .map(|k| Key {
                    frame: k.frame,
                    value: k.value,
                })
                .collect(),
            segments,
        }
    }

    /// A track that holds `value` at every frame.
    pub fn constant(value: Value) -> Track {
        Track::new(
            value.value_type(),
            vec![KeySpec::linear(0.0, value)],
            1.0,
            false,
        )
    }

    /// The type of its values.
    pub fn value_type(&self) -> ValueType {
        self.ty
    }

    /// The keys, sorted by frame.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Whether the value is the same at every frame.
    pub fn is_constant(&self) -> bool {
        // A span between equal keys can still move (a speed or a bent path).
        self.keys.iter().all(|k| k.value == self.keys[0].value)
            && self.segments.iter().all(Segment::is_still)
    }

    /// The value at `frame`, held past the ends. Of several keys on one
    /// frame, the last applies from that frame on.
    pub fn sample(&self, frame: f64) -> Value {
        // Lands after every key of a shared frame, so the last of them wins.
        let after = self.keys.partition_point(|k| k.frame <= frame);
        let Some(i) = after.checked_sub(1) else {
            return self.keys[0].value;
        };
        let (left, Some(right)) = (&self.keys[i], self.keys.get(after)) else {
            return self.keys[i].value;
        };
        // right.frame > frame >= left.frame, so the span is positive.
        let p = (frame - left.frame) / (right.frame - left.frame);
        self.segments[i].sample(left.value, right.value, p)
    }
}

impl Segment {
    /// The value at progress `p` from `from` to `to`.
    fn sample(&self, from: Value, to: Value, p: f64) -> Value {
        match self {
            Segment::Components(eases) => {
                let components: Vec<f64> = eases
                    .iter()
                    .zip(from.components().iter().zip(to.components()))
                    .map(|(ease, (&a, &b))| ease.sample(a, b, p))
                    .collect();
                Value::from_components(from.value_type(), &components)
            }
            Segment::Path { path, length, ease } => {
                let along = ease.sample(0.0, *length, p);
                let point = match *length {
                    0.0 => path.p0,
                    // Beyond either end (an overshooting ease) the path carries on straight.
                    l if along <= 0.0 || along >= l => extend(path, along, l),
                    _ => path.eval(path.inv_arclen(along, ARC_ACCURACY)),
                };
                Value::Point([point.x, point.y])
            }
        }
    }

    /// Whether the span never leaves its first value when both keys are equal.
    fn is_still(&self) -> bool {
        match self {
            Segment::Components(eases) => eases.iter().all(|e| match e {
                Ease::Bezier { out, into } => out.1 == 0.0 && into.1 == 0.0,
                Ease::Linear | Ease::Hold => true,
            }),
            Segment::Path { length, .. } => *length == 0.0,
        }
    }
}

/// The span from `a` to `b`.
fn segment(a: &KeySpec, b: &KeySpec, fps: f64, spatial: bool) -> Segment {
    let seconds = (b.frame - a.frame) / fps;
    if spatial {
        let (p0, p3) = (point(a.value), point(b.value));
        let [out, into] = a.path.unwrap_or([[0.0; 2]; 2]);
        let path = CubicBez::new(
            p0,
            p0 + kurbo::Vec2::new(out[0], out[1]),
            p3 + kurbo::Vec2::new(into[0], into[1]),
            p3,
        );
        let length = path.arclen(ARC_ACCURACY);
        return Segment::Path {
            path,
            length,
            ease: Ease::from_curve(a.easing.of(0), length, seconds),
        };
    }
    let deltas = a
        .value
        .components()
        .iter()
        .zip(b.value.components())
        .map(|(x, y)| y - x);
    Segment::Components(
        deltas
            .enumerate()
            .map(|(i, delta)| Ease::from_curve(a.easing.of(i), delta, seconds))
            .collect(),
    )
}

fn point(value: Value) -> Point {
    let [x, y] = value.as_point().expect("a spatial track holds points");
    Point::new(x, y)
}

/// A point `along` pixels from the start of `path` when that lies before its
/// start or past its end: the path's end tangent carried on in a straight line.
fn extend(path: &CubicBez, along: f64, length: f64) -> Point {
    let (end, toward, beyond) = if along <= 0.0 {
        (path.p0, path.p0 - tangent_at(path, 0.0), -along)
    } else {
        (path.p3, path.p3 + tangent_at(path, 1.0), along - length)
    };
    let direction = toward - end;
    match direction.hypot() {
        0.0 => end,
        n => end + direction * (beyond / n),
    }
}

/// The direction of travel at `t`; at an end with a zero handle, toward the other handle.
fn tangent_at(path: &CubicBez, t: f64) -> kurbo::Vec2 {
    let candidates = if t == 0.0 {
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
#[path = "../tests/unit/track.rs"]
mod tests;
