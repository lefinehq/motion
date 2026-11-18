//! Values and keyframes, from the project's units to the scene's: a static
//! value becomes a resting value, keyframes an `anim` key list whose curves
//! carry the project's eases as `speed(…)`.

use serde_json::{Value as Json, json};

use nakkash_format::Curve;

use crate::eval::Depth;
use crate::model::{Color, Interpolation, Keyframe, Path, Stream, Value, Vertex};

/// How a project value becomes a scene value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Convert {
    /// A number, as it is.
    Number,
    /// A number in percent, to a fraction.
    Percent,
    /// An 8-bit level, 0 to 255, to a fraction.
    Level,
    /// A number in an effect's own units, times the scene's units per one.
    Times(f64),
    /// A 2D or 3D vector, to a point; a 3D vector's depth is dropped.
    Point,
    /// The third number of a 3D vector, as it is: the depth of a point that
    /// [`Convert::Point`] writes, or a scale's factor along depth.
    Depth,
    /// Three angles in degrees, as they are: an orientation, which turns
    /// by one ease over the angle between its keys.
    Angles,
    /// A vector in percent, to a point of fractions.
    PointPercent,
    /// A point in fractions of a layer's effect space, as an effect stores
    /// its points, to pixels of the layer: times the space's `size`, less
    /// where the layer's own origin sits in that space, its `origin`.
    Sized {
        /// The effect space's size: the source's, else the composition's.
        size: [f64; 2],
        /// The layer's origin in the effect space: the top left for a
        /// layer with a source, the middle for a shape or text layer.
        origin: [f64; 2],
    },
    /// A colour, to `[r, g, b, 1]`: the project draws every colour it
    /// stores opaque, whatever alpha the colour carries, and keeps opacity
    /// in properties of its own.
    Color,
    /// A bezier path, to SVG path data.
    Path,
    /// A bezier path in a space of this size per unit, as a mask stores its
    /// path, to SVG path data in px.
    PathIn([f64; 2]),
}

impl Convert {
    /// The factor from the project's units to the scene's for the
    /// component `dim`, which speeds take too.
    pub(crate) fn factor(self, dim: usize) -> f64 {
        match self {
            Convert::Percent | Convert::PointPercent => 0.01,
            Convert::Level => 1.0 / 255.0,
            Convert::Times(k) => k,
            Convert::Sized { size, .. } => size[dim.min(1)],
            _ => 1.0,
        }
    }

    /// What the scene takes away from the project's value times
    /// [`Convert::factor`] for the component `dim`; speeds ignore it.
    pub(crate) fn offset(self, dim: usize) -> f64 {
        match self {
            Convert::Sized { origin, .. } => origin[dim.min(1)],
            _ => 0.0,
        }
    }

    /// How one component of a value of this kind becomes a scene number.
    pub(super) fn component(self, dim: usize) -> Convert {
        match self {
            Convert::Point => Convert::Number,
            Convert::PointPercent => Convert::Percent,
            Convert::Sized { size, origin } => Convert::Sized {
                size: [size[dim.min(1)]; 2],
                origin: [origin[dim.min(1)]; 2],
            },
            other => other,
        }
    }

    /// The first dimension of the project's value this converts, whose
    /// eases its curves take: the depth is the third.
    fn first_dim(self) -> usize {
        match self {
            Convert::Depth => 2,
            _ => 0,
        }
    }

    /// How many components the scene value has, for eases per component.
    fn dims(self) -> usize {
        match self {
            Convert::Point | Convert::PointPercent | Convert::Sized { .. } => 2,
            _ => 1,
        }
    }

    /// The scene value of `value`, or `None` when it is not of this kind.
    pub(crate) fn value(self, value: &Value) -> Option<Json> {
        let scene = |n: f64, dim| n * self.factor(dim) - self.offset(dim);
        match (self, value) {
            (
                Convert::Number
                | Convert::Percent
                | Convert::Level
                | Convert::Times(_)
                | Convert::Sized { .. },
                Value::Number(n),
            ) => Some(num(scene(*n, 0))),
            (
                Convert::Point | Convert::PointPercent | Convert::Sized { .. },
                Value::Vector2([x, y]) | Value::Vector3([x, y, _]),
            ) => Some(json!([num(scene(*x, 0)), num(scene(*y, 1))])),
            (Convert::Depth, Value::Vector3([_, _, z]) | Value::Number(z)) => Some(num(*z)),
            (Convert::Angles, Value::Vector3(angles)) => Some(json!(angles.map(num))),
            (Convert::Color, Value::Color(color)) => Some(color_json(&Color { a: 1.0, ..*color })),
            (Convert::Path, Value::Path(path)) => Some(Json::String(svg(path))),
            (Convert::PathIn(size), Value::Path(path)) => {
                Some(Json::String(svg(&path_scaled(path, size))))
            }
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

/// `path` with every point and tangent scaled by `size`, per axis.
pub(super) fn path_scaled(path: &Path, [w, h]: [f64; 2]) -> Path {
    let scale = |[x, y]: [f64; 2]| [x * w, y * h];
    Path {
        vertices: path
            .vertices
            .iter()
            .map(|v| Vertex {
                point: scale(v.point),
                in_tangent: scale(v.in_tangent),
                out_tangent: scale(v.out_tangent),
            })
            .collect(),
        ..path.clone()
    }
}

/// The paths of a mask's keys, closed, each with as many vertices as the
/// one with the most, as the scene's path keys must be. A path with fewer
/// repeats its first vertex at its end: the segment that closed it is drawn
/// as before, and the one from the copy back to the first has no length.
fn matched(keys: &[Keyframe]) -> Vec<Value> {
    let most = keys
        .iter()
        .filter_map(|k| match &k.value {
            Value::Path(path) => Some(path.vertices.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    keys.iter()
        .map(|key| match &key.value {
            Value::Path(path) if !path.vertices.is_empty() => {
                let mut vertices = path.vertices.clone();
                let first = vertices[0];
                vertices.resize(
                    most,
                    Vertex {
                        out_tangent: [0.0, 0.0],
                        ..first
                    },
                );
                Value::Path(Path {
                    closed: true,
                    vertices,
                })
            }
            other => other.clone(),
        })
        .collect()
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

/// What clamping a written value into a range changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Clamped {
    /// How many values or keys were outside it.
    pub count: usize,
    /// Whether a clamped key's neighbouring motion is a curve, not a hold,
    /// so the motion between keys can differ from the original's, which
    /// interpolates first and clamps after.
    pub between_keys: bool,
}

impl Written {
    /// Clamps every value and key into `range` (inclusive), as the original
    /// draws a value outside its property's range.
    pub(super) fn clamp(&mut self, range: (f64, f64)) -> Clamped {
        match self {
            Written::Value(value) => Clamped {
                count: usize::from(clamp_json(value, range)),
                between_keys: false,
            },
            Written::Track { keys, first } => {
                clamp_json(first, range);
                let Json::Array(keys) = keys else {
                    return Clamped::default();
                };
                let holds: Vec<bool> = keys
                    .iter()
                    .map(|k| k.get("curve").and_then(Json::as_str) == Some("hold"))
                    .collect();
                let mut clamped = Clamped::default();
                for (i, key) in keys.iter_mut().enumerate() {
                    let Some(value) = key.get_mut("v") else {
                        continue;
                    };
                    if clamp_json(value, range) {
                        clamped.count += 1;
                        let leaving = i + 1 < holds.len() && !holds[i];
                        let arriving = i > 0 && !holds[i - 1];
                        clamped.between_keys |= leaving || arriving;
                    }
                }
                clamped
            }
        }
    }
}

/// Clamps the numbers of `value` into `range`; whether any was outside it.
fn clamp_json(value: &mut Json, (lo, hi): (f64, f64)) -> bool {
    match value {
        Json::Number(n) => match n.as_f64() {
            Some(x) if x < lo || x > hi => {
                *value = num(x.clamp(lo, hi));
                true
            }
            _ => false,
        },
        Json::Array(items) => items
            .iter_mut()
            .fold(false, |any, item| clamp_json(item, (lo, hi)) | any),
        _ => false,
    }
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
            let matched = matches!(convert, Convert::PathIn(_)).then(|| matched(keys));
            let values: Vec<Json> = match &matched {
                Some(paths) => paths
                    .iter()
                    .map(|p| convert.value(p))
                    .collect::<Option<_>>(),
                None => keys.iter().map(|k| convert.value(&k.value)).collect(),
            }?;
            track(keys, &values, convert, spatial, clock)
        }
    }
}

/// A key list for `anim` with `values` at the times of `keys` and the
/// curves between them, or `None` without keys. `spatial` tracks take one
/// curve and keep their motion-path tangents.
pub(super) fn track(
    keys: &[Keyframe],
    values: &[Json],
    convert: Convert,
    spatial: bool,
    clock: Clock,
) -> Option<Written> {
    let first = values.first()?.clone();
    let written: Vec<Json> = keys
        .iter()
        .zip(values)
        .enumerate()
        .map(|(i, (key, value))| {
            let mut object = serde_json::Map::new();
            object.insert("t".into(), Json::String(clock.time(key.time)));
            object.insert("v".into(), value.clone());
            if let Some(next) = keys.get(i + 1) {
                object.insert("curve".into(), curve(key, next, convert, spatial, clock));
                if let Some(path) = spatial.then(|| bend(key, next, convert)).flatten() {
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

/// A stream of 3D vectors as a point and its depth: the point through
/// `convert`, the depth through [`Convert::Depth`], or `None` for the depth
/// where the stream holds no third number. `spatial` streams move along a
/// path in space ([`spatial_split`]); the others key each part as it is,
/// the depth with its own ease.
pub(super) fn split(
    stream: &Stream,
    convert: Convert,
    spatial: bool,
    clock: Clock,
    frame_rate: f64,
) -> Option<(Written, Option<Written>)> {
    match (stream, spatial) {
        (Stream::Animated(keys), true) => spatial_split(stream, keys, convert, clock, frame_rate),
        _ => Some((
            written(stream, convert, spatial, clock)?,
            written(stream, Convert::Depth, false, clock),
        )),
    }
}

/// Keys of points with depth that move along a path in space, as a
/// spatial track of the point and a track of its depth. A span that stays
/// at one depth keeps its flat path; a straight one keeps its ease, its
/// speed along the line in space shared out to the point and the depth by
/// how far each moves; a span whose path bends in depth is keyed at every
/// frame of the composition between its keys, from our evaluation of the
/// path ([`crate::eval::Path3`]), as the scene's paths are flat.
fn spatial_split(
    stream: &Stream,
    keys: &[Keyframe],
    convert: Convert,
    clock: Clock,
    frame_rate: f64,
) -> Option<(Written, Option<Written>)> {
    if !keys.iter().all(|k| matches!(k.value, Value::Vector3(_))) {
        return Some((written(stream, convert, true, clock)?, None));
    }
    let stretch = clock.stretch.abs();
    let factor = f64::midpoint(convert.factor(0), convert.factor(1)) * stretch;
    let key = |time: f64, value: Json, curve: Option<Json>, path: Option<Json>| {
        let mut object = serde_json::Map::new();
        object.insert("t".into(), Json::String(clock.time(time)));
        object.insert("v".into(), value);
        if let Some(curve) = curve {
            object.insert("curve".into(), curve);
        }
        if let Some(path) = path {
            object.insert("path".into(), path);
        }
        Json::Object(object)
    };
    let linear = || Some(Json::String(curve_text(Curve::Linear)));
    let (mut points, mut depths) = (Vec::new(), Vec::new());
    let mut bends = false;
    for (i, at) in keys.iter().enumerate() {
        let (point, depth) = (convert.value(&at.value)?, Convert::Depth.value(&at.value)?);
        let Some(next) = keys.get(i + 1) else {
            points.push(key(at.time, point, None, None));
            depths.push(key(at.time, depth, None, None));
            break;
        };
        let held = at.out_interpolation == Interpolation::Hold;
        match crate::eval::depth(at, next) {
            _ if held => {
                let hold = Some(Json::String(curve_text(Curve::Hold)));
                points.push(key(at.time, point, hold.clone(), None));
                depths.push(key(at.time, depth, hold, None));
            }
            Depth::Level => {
                let curve = curve(at, next, convert, true, clock);
                points.push(key(at.time, point, Some(curve), bend(at, next, convert)));
                depths.push(key(at.time, depth, linear(), None));
            }
            Depth::Straight => {
                let [from, to] = [at, next].map(|k| k.value.numbers().unwrap_or_default());
                let moved = |dims: std::ops::Range<usize>| {
                    dims.map(|d| (to[d] - from[d]).powi(2)).sum::<f64>().sqrt()
                };
                let (flat, space) = (moved(0..2), moved(0..3));
                // The share of the speed along the line each part takes.
                let share = |part: f64| if space > 0.0 { part / space } else { 0.0 };
                let ease = crate::eval::curve(at, next, 0);
                let point_curve = curves(std::iter::once(scaled(ease, factor * share(flat))));
                let depth_curve = curves(std::iter::once(scaled(
                    ease,
                    stretch * share(to[2] - from[2]),
                )));
                points.push(key(at.time, point, Some(point_curve), None));
                depths.push(key(at.time, depth, Some(depth_curve), None));
            }
            Depth::Bent => {
                bends = true;
                points.push(key(at.time, point, linear(), None));
                depths.push(key(at.time, depth, linear(), None));
                for time in frames_between(at.time, next.time, frame_rate) {
                    let value = stream.value_at(time);
                    points.push(key(time, convert.value(&value)?, linear(), None));
                    depths.push(key(time, Convert::Depth.value(&value)?, linear(), None));
                }
            }
        }
    }
    let track = |keys: Vec<Json>| {
        let first = keys.first()?.get("v")?.clone();
        Some(Written::Track {
            keys: Json::Array(keys),
            first,
        })
    };
    // A depth that never moves is one value.
    let first_depth = depths.first()?.get("v")?.clone();
    let still = !bends && depths.iter().all(|k| k.get("v") == Some(&first_depth));
    let depth = match still {
        true => Written::Value(first_depth),
        false => track(depths)?,
    };
    Some((track(points)?, Some(depth)))
}

/// The times of a composition's frames strictly between `from` and `to`, in
/// seconds, at `frame_rate`.
fn frames_between(from: f64, to: f64, frame_rate: f64) -> impl Iterator<Item = f64> {
    let rate = frame_rate.max(1.0);
    // A frame closer to a key than this is the key's own.
    let near = 1e-6;
    #[expect(clippy::cast_possible_truncation, reason = "frames of a composition")]
    let (first, last) = (
        (from * rate).floor() as i64 + 1,
        (to * rate).ceil() as i64 - 1,
    );
    (first..=last)
        .map(move |n| n as f64 / rate)
        .filter(move |&t| t > from + near && t < to - near)
}

/// The curve from `key` to `next` ([`crate::eval::curve`]), one per component
/// where the project keeps one per dimension, with speeds per second of the
/// layer's own time, which a stretch slows.
fn curve(key: &Keyframe, next: &Keyframe, convert: Convert, spatial: bool, clock: Clock) -> Json {
    let stretch = clock.stretch.abs();
    match spatial {
        // One speed along the path: its components share a factor but for
        // a point in fractions of a size that is not square, where their
        // mean stands in.
        true => {
            let factor = f64::midpoint(convert.factor(0), convert.factor(1)) * stretch;
            curves(std::iter::once(scaled(
                crate::eval::curve(key, next, 0),
                factor,
            )))
        }
        false => curves((0..convert.dims()).map(|dim| {
            scaled(
                crate::eval::curve(key, next, convert.first_dim() + dim),
                convert.factor(dim) * stretch,
            )
        })),
    }
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
        // A project's keys are béziers and holds; these only print as written.
        Curve::Shaped(..) | Curve::Steps { .. } | Curve::Spring(_) => curve.to_string(),
    }
}

/// The motion path's bend from `key` to `next`: the handle leaving `key` and
/// the one arriving at `next`, each from its key's value, in the scene's
/// units through `convert`, or `None` when both are zero and the path is
/// straight.
fn bend(key: &Keyframe, next: &Keyframe, convert: Convert) -> Option<Json> {
    let handle = |tangent: Option<&Vec<f64>>| {
        let t = tangent.map(Vec::as_slice).unwrap_or_default();
        [
            t.first().copied().unwrap_or(0.0) * convert.factor(0),
            t.get(1).copied().unwrap_or(0.0) * convert.factor(1),
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
    channels: &[nakkash_expr::fit::Fitted],
    template: &Value,
    convert: Convert,
    clock: Clock,
) -> Option<Written> {
    let count = channels.first()?.keys.len();
    let factor = |dim: usize| convert.factor(dim) * clock.stretch;
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
            let curve =
                match channels[0].keys[k].hold {
                    true => curve_text(Curve::Hold).into(),
                    false => curves(channels.iter().take(scene_dims).enumerate().map(
                        |(dim, c)| Curve::Speed {
                            out_speed: c.keys[k].slope_out * factor(dim),
                            out_reach: FITTED_REACH,
                            in_speed: c.keys[k + 1].slope_in * factor(dim),
                            in_reach: FITTED_REACH,
                        },
                    )),
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

/// Samples at `times`, one channel per number of `template`'s kind, as a
/// track keyed at each of them, straight from one to the next.
pub(super) fn every_frame(
    times: &[f64],
    channels: &[Vec<f64>],
    template: &Value,
    convert: Convert,
    clock: Clock,
) -> Option<Written> {
    let keys: Vec<Json> = times
        .iter()
        .enumerate()
        .map(|(i, &time)| {
            let numbers: Vec<f64> = channels
                .iter()
                .map(|c| c.get(i).copied())
                .collect::<Option<_>>()?;
            let mut object = serde_json::Map::new();
            object.insert("t".into(), Json::String(clock.time(time)));
            object.insert("v".into(), convert.value(&template.with_numbers(&numbers))?);
            if i + 1 < times.len() {
                object.insert("curve".into(), Json::String(curve_text(Curve::Linear)));
            }
            Some(Json::Object(object))
        })
        .collect::<Option<_>>()?;
    let first = keys.first()?.get("v")?.clone();
    Some(Written::Track {
        keys: Json::Array(keys),
        first,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/map/keys.rs"]
mod tests;
