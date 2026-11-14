//! Values and keyframes, from the project's units to the scene's: a static
//! value becomes a resting value, keyframes an `anim` key list whose curves
//! carry the project's eases as `speed(…)`.

use serde_json::{Value as Json, json};

use motion_format::Curve;

use crate::model::{Color, Keyframe, Path, Stream, Value};

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

    /// How one component of a value of this kind becomes a scene number.
    pub(super) fn component(self) -> Convert {
        match self {
            Convert::Point => Convert::Number,
            Convert::PointPercent => Convert::Percent,
            other => other,
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

/// The curve from `key` to `next` ([`crate::eval::curve`]), one per component
/// where the project keeps one per dimension, with speeds per second of the
/// layer's own time, which a stretch slows.
fn curve(key: &Keyframe, next: &Keyframe, convert: Convert, spatial: bool, clock: Clock) -> Json {
    let factor = convert.factor() * clock.stretch.abs();
    let dims = if spatial { 1 } else { convert.dims() };
    curves((0..dims).map(|dim| scaled(crate::eval::curve(key, next, dim), factor)))
}

/// `curve` with its speeds times `factor`.
fn scaled(curve: Curve, factor: f64) -> Curve {
    match curve {
        Curve::Speed {
            out_speed,
            out_reach,
            in_speed,
            in_reach,
        } => Curve::Speed {
            out_speed: out_speed * factor,
            out_reach,
            in_speed: in_speed * factor,
            in_reach,
        },
        other => other,
    }
}

/// A key's `curve`: one curve when every component shares it, else one per
/// component.
fn curves(each: impl Iterator<Item = Curve>) -> Json {
    let texts: Vec<String> = each.map(curve_text).collect();
    match texts.iter().all(|c| *c == texts[0]) {
        true => Json::String(texts[0].clone()),
        false => json!(texts),
    }
}

/// A curve as the scene spells it, its numbers at the import's precision.
/// (The format's own printer rounds for people to read.)
fn curve_text(curve: Curve) -> String {
    match curve {
        Curve::Linear => "linear".into(),
        Curve::Hold => "hold".into(),
        Curve::Speed {
            out_speed,
            out_reach,
            in_speed,
            in_reach,
        } => format!(
            "speed({},{},{},{})",
            text(out_speed),
            text(out_reach),
            text(in_speed),
            text(in_reach)
        ),
        Curve::Bezier(x1, y1, x2, y2) => {
            format!("cubic({},{},{},{})", text(x1), text(y1), text(x2), text(y2))
        }
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

/// How far a fitted key's handles reach into its span, which makes the
/// scene's cubic in time the fitter's Hermite curve.
const FITTED_REACH: f64 = 1.0 / 3.0;

/// Channels fitted with shared key times as a scene track. `template` is a
/// value of the property's kind, which each key's numbers fill; slopes are
/// per composition second and become speeds per second of the layer's own
/// time. Each scene component gets the curve of its channel: a point's
/// first two, a colour's four, a number's one.
pub(super) fn fitted(
    channels: &[motion_expr::fit::Fitted],
    template: &Value,
    convert: Convert,
    clock: Clock,
) -> Option<Written> {
    let count = channels.first()?.keys.len();
    let factor = convert.factor() * clock.stretch;
    let scene_dims = match template {
        Value::Color(_) => 4,
        _ => convert.dims(),
    };
    let mut written = Vec::with_capacity(count);
    let mut first = None;
    for k in 0..count {
        let numbers: Vec<f64> = channels.iter().map(|c| c.keys[k].value).collect();
        let value = convert.value(&template.with_numbers(&numbers))?;
        first.get_or_insert_with(|| value.clone());
        let mut object = serde_json::Map::new();
        object.insert(
            "t".into(),
            Json::String(clock.time(channels[0].keys[k].time)),
        );
        object.insert("v".into(), value);
        if k + 1 < count {
            let curve = match channels[0].keys[k].hold {
                true => curve_text(Curve::Hold).into(),
                false => curves(channels.iter().take(scene_dims).map(|c| Curve::Speed {
                    out_speed: c.keys[k].slope_out * factor,
                    out_reach: FITTED_REACH,
                    in_speed: c.keys[k + 1].slope_in * factor,
                    in_reach: FITTED_REACH,
                })),
            };
            object.insert("curve".into(), curve);
        }
        written.push(Json::Object(object));
    }
    Some(Written::Track {
        keys: Json::Array(written),
        first: first?,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/map/keys.rs"]
mod tests;
