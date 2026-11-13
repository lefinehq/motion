//! The compiler: scene in, scene graph out.
//!
//! Part of step 2 (Compile). It resolves `center` coordinates, measures text,
//! and turns each layer's entrance and `anim` keys into one keyframe track per
//! property, with times in frames.

use motion_format::preset::Start;
use motion_format::property::names;
use motion_format::property::{PROPERTIES, property};
use motion_format::shorthand::Coord;
use motion_format::{Content, Curve, Layer, Scene};

use crate::graph::{Node, NodeKind, SceneGraph};
use crate::text::TextMeasure;
use crate::track::{Key, Track};

/// Compiles `scene` into a [`SceneGraph`], using `measure` to size text.
pub fn compile(scene: &Scene, measure: &dyn TextMeasure) -> SceneGraph {
    let width = f64::from(scene.size.width);
    let height = f64::from(scene.size.height);
    let nodes = scene
        .layers
        .iter()
        .map(|layer| {
            let (kind, w, h) = match &layer.content {
                Content::Text { text, font } => {
                    let (w, h) = measure.measure(text, font);
                    (NodeKind::Text, w, h)
                }
                Content::Rect(r) => (NodeKind::Rect, r.width, r.height),
            };
            let (tracks, follows_at) = tracks(layer, scene, (width, height));
            Node {
                id: layer.id.clone(),
                kind,
                width: w,
                height: h,
                tracks,
                follows_at,
            }
        })
        .collect();
    SceneGraph {
        width,
        height,
        fps: scene.fps,
        duration_frames: scene.duration.to_frames(scene.fps),
        nodes,
    }
}

/// Turns a coordinate into pixels; `center` is half the canvas extent.
fn resolve(c: Coord, extent: f64) -> f64 {
    match c {
        Coord::Center => extent / 2.0,
        Coord::Px(v) => v,
    }
}

/// Builds the tracks of `layer`, parallel to [`PROPERTIES`], and whether
/// `(x, y)` still follow `at`.
fn tracks(layer: &Layer, scene: &Scene, canvas: (f64, f64)) -> (Vec<Track>, (bool, bool)) {
    let fps = scene.fps;
    let index_of = |name: &str| property(name).map(|(i, _)| i);
    // `at` sets the resting x and y.
    let (x_at, y_at) = (index_of(names::X), index_of(names::Y));
    // Resting values: `at` for the two position properties, the registry
    // default for the rest.
    let rest: Vec<f64> = PROPERTIES
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            if Some(i) == x_at {
                resolve(layer.at.x, canvas.0)
            } else if Some(i) == y_at {
                resolve(layer.at.y, canvas.1)
            } else {
                spec.default
            }
        })
        .collect();
    let mut tracks: Vec<Track> = rest.iter().map(|&v| Track::constant(v)).collect();

    // The entrance: every property the preset names runs from its start
    // value to its resting value.
    if let Some(b) = &layer.enter
        && let Some(spec) = scene.preset(&b.preset)
    {
        let start = b.start.to_frames(fps);
        let end = start + b.duration.to_frames(fps);
        for &(prop, from) in &spec.props {
            let rest = rest[prop];
            let from = match from {
                Start::Value(v) => v,
                Start::Plus => rest + b.distance,
                Start::Minus => rest - b.distance,
            };
            tracks[prop] = Track::new(vec![
                Key {
                    frame: start,
                    value: from,
                    curve: b.curve,
                },
                Key {
                    frame: end,
                    value: rest,
                    curve: Curve::Linear,
                },
            ]);
        }
    }

    // `anim` wins over the entrance for the properties it names.
    for a in &layer.anim {
        let keys = a
            .keys
            .iter()
            .map(|k| Key {
                frame: k.time.to_frames(fps),
                value: k.value,
                curve: k.curve,
            })
            .collect();
        tracks[a.property] = Track::new(keys);
    }
    let follows = |axis: Option<usize>| !layer.anim.iter().any(|a| Some(a.property) == axis);
    (tracks, (follows(x_at), follows(y_at)))
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
