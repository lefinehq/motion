//! Values and keyframes, from the project's units to the scene's: a static
//! value becomes a resting value, keyframes an `anim` key list whose curves
//! carry the project's eases as `speed(…)`.

use serde_json::{Value as Json, json};

use crate::model::{Color, Interpolation, Keyframe, Path, Stream, Value};

/// How a project value becomes a scene value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Convert {
    /// A number, as it is.
    Number,
    /// A number in percent, to a fraction.
    Percent,
    /// A 2D or 3D vector, to a point; a 3D vector's depth is dropped.
    Point,
    /// A vector in percent, to a point of fractions.
    PointPercent,
    /// A colour, to `[r, g, b, a]`.
    Color,
    /// A bezier path, to SVG path data.
    Path,
}

impl Convert {
    /// The factor from the project's units to the scene's, which speeds take too.
    fn factor(self) -> f64 {
        match self {
            Convert::Percent | Convert::PointPercent => 0.01,
            _ => 1.0,
        }
    }

    /// How many components the scene value has, for eases per component.
    fn dims(self) -> usize {
        match self {
            Convert::Point | Convert::PointPercent => 2,
            _ => 1,
        }
    }

    /// The scene value of `value`, or `None` when it is not of this kind.
    pub(super) fn value(self, value: &Value) -> Option<Json> {
        let factor = self.factor();
        match (self, value) {
            (Convert::Number | Convert::Percent, Value::Number(n)) => Some(num(n * factor)),
            (
                Convert::Point | Convert::PointPercent,
                Value::Vector2([x, y]) | Value::Vector3([x, y, _]),
            ) => Some(json!([num(x * factor), num(y * factor)])),
            (Convert::Color, Value::Color(color)) => Some(color_json(color)),
            (Convert::Path, Value::Path(path)) => Some(Json::String(svg(path))),
            _ => None,
        }
    }
}

/// A number rounded to six decimals, which drops the binary noise of the
/// project's doubles (`511.4999999999902`) and keeps every visible digit.
pub(super) fn round(n: f64) -> f64 {
    let rounded = (n * 1e6).round() / 1e6;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// A JSON number, rounded, written whole when it is whole.
pub(super) fn num(n: f64) -> Json {
    let n = round(n);
    #[expect(clippy::cast_possible_truncation, reason = "whole and far below 2^53")]
    match n.fract() == 0.0 && n.abs() < 1e15 {
        true => json!(n as i64),
        false => json!(n),
    }
}

/// A number as text, rounded, for the curves and times inside strings.
fn text(n: f64) -> String {
    num(n).to_string()
}

/// A colour as `[r, g, b, a]`.
pub(super) fn color_json(color: &Color) -> Json {
    json!([num(color.r), num(color.g), num(color.b), num(color.a)])
}

/// A path as absolute SVG path data: a cubic for every segment, so every key
/// of a path track has as many commands as the path has vertices.
pub(super) fn svg(path: &Path) -> String {
    let Some(first) = path.vertices.first() else {
        return String::new();
    };
    let point = |[x, y]: [f64; 2]| format!("{},{}", text(x), text(y));
    let add = |[x, y]: [f64; 2], [dx, dy]: [f64; 2]| [x + dx, y + dy];
    let next = path.vertices.iter().skip(1);
    let closing = path.closed.then_some(first);
    let segments: Vec<String> = path
        .vertices
        .iter()
        .zip(next.chain(closing))
        .map(|(from, to)| {
            format!(
                "C{} {} {}",
                point(add(from.point, from.out_tangent)),
                point(add(to.point, to.in_tangent)),
                point(to.point)
            )
        })
        .collect();
    let close = if path.closed { " Z" } else { "" };
    format!("M{} {}{close}", point(first.point), segments.join(" "))
}

/// Where a layer's own time 0 falls on its composition's clock, and how fast
/// its own time runs: keys are written in the layer's own seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Clock {
    /// The layer's start time, in composition seconds.
    pub start: f64,
    /// The layer's stretch: 2 runs its time at half speed.
    pub stretch: f64,
}

impl Clock {
    /// The layer's own time at composition time `seconds`, undoing the
    /// decoder's mapping (a reversed layer's time 0 sits a 3000th of its
    /// stretch before its start).
    pub(super) fn local(self, seconds: f64) -> f64 {
        (seconds - self.start - self.stretch.min(0.0) / 3000.0) / self.stretch
    }

    /// A time as the scene writes it, in the layer's own seconds.
    pub(super) fn time(self, seconds: f64) -> String {
        format!("{}s", text(self.local(seconds)))
    }
}

/// A stream as the scene writes it.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Written {
    /// One value at all times.
    Value(Json),
    /// A key list for `anim`, and the first key's value, which a kind key
    /// that holds the property needs.
    Track {
        /// The key list.
        keys: Json,
        /// The first key's value.
        first: Json,
    },
}

/// `stream` in the scene's units, or `None` when a value is not of the kind
/// `convert` expects. `spatial` streams take one curve and keep their
/// motion-path tangents.
pub(super) fn written(
    stream: &Stream,
    convert: Convert,
    spatial: bool,
    clock: Clock,
) -> Option<Written> {
    match stream {
        Stream::Static(value) => convert.value(value).map(Written::Value),
        Stream::Animated(keys) => {
            let values: Vec<Json> = keys
                .iter()
                .map(|k| convert.value(&k.value))
                .collect::<Option<_>>()?;
            let first = values.first()?.clone();
            let written: Vec<Json> = keys
                .iter()
                .zip(&values)
                .enumerate()
                .map(|(i, (key, value))| {
                    let mut object = serde_json::Map::new();
                    object.insert("t".into(), Json::String(clock.time(key.time)));
                    object.insert("v".into(), value.clone());
                    if let Some(next) = keys.get(i + 1) {
                        object.insert("curve".into(), curve(key, next, convert, spatial, clock));
                        if let Some(path) = spatial.then(|| bend(key, next)).flatten() {
                            object.insert("path".into(), path);
                        }
                    }
                    Json::Object(object)
                })
                .collect();
            Some(Written::Track {
                keys: Json::Array(written),
                first,
            })
        }
    }
}

/// The curve from `key` to `next`: `hold`, `linear`, or the eases on both
/// sides as `speed(…)`, one per component where the project keeps one per
/// dimension. A linear side reaches nowhere into the span, as in the project.
fn curve(key: &Keyframe, next: &Keyframe, convert: Convert, spatial: bool, clock: Clock) -> Json {
    if key.out_interpolation == Interpolation::Hold {
        return Json::String("hold".into());
    }
    let linear = |i: Interpolation| i == Interpolation::Linear;
    if linear(key.out_interpolation) && linear(next.in_interpolation) {
        return Json::String("linear".into());
    }
    // Speeds are per second of the layer's own time, which a stretch slows.
    let factor = convert.factor() * clock.stretch.abs();
    let side = |eases: &[crate::model::Ease], dim: usize, interpolation: Interpolation| match (
        linear(interpolation),
        eases.get(dim).or_else(|| eases.first()),
    ) {
        (false, Some(ease)) => (ease.speed * factor, ease.influence.clamp(0.0, 1.0)),
        _ => (0.0, 0.0),
    };
    let speed = |dim: usize| {
        let (out_speed, out_reach) = side(&key.out_ease, dim, key.out_interpolation);
        let (in_speed, in_reach) = side(&next.in_ease, dim, next.in_interpolation);
        format!(
            "speed({},{},{},{})",
            text(out_speed),
            text(out_reach),
            text(in_speed),
            text(in_reach)
        )
    };
    let dims = if spatial { 1 } else { convert.dims() };
    let curves: Vec<String> = (0..dims).map(speed).collect();
    match curves.iter().all(|c| *c == curves[0]) {
        true => Json::String(curves[0].clone()),
        false => json!(curves),
    }
}

/// The motion path's bend from `key` to `next`: the handle leaving `key` and
/// the one arriving at `next`, each from its key's value, or `None` when both
/// are zero and the path is straight.
fn bend(key: &Keyframe, next: &Keyframe) -> Option<Json> {
    let handle = |tangent: Option<&Vec<f64>>| {
        let t = tangent.map(Vec::as_slice).unwrap_or_default();
        [
            t.first().copied().unwrap_or(0.0),
            t.get(1).copied().unwrap_or(0.0),
        ]
    };
    let out = handle(key.spatial.as_ref().map(|s| &s.out_tangent));
    let arrive = handle(next.spatial.as_ref().map(|s| &s.in_tangent));
    let zero = |[x, y]: [f64; 2]| round(x) == 0.0 && round(y) == 0.0;
    (!zero(out) || !zero(arrive))
        .then(|| json!([[num(out[0]), num(out[1])], [num(arrive[0]), num(arrive[1])]]))
}

#[cfg(test)]
#[path = "../../tests/unit/map/keys.rs"]
mod tests;
