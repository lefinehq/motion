//! Step 2 (Compile): resolves positions, measures text and turns each layer's
//! entrance and `anim` into one keyframe track per property.

use motion_format::anim::AnimTrack;
use motion_format::preset::{PresetSpec, Start};
use motion_format::property::{PROPERTIES, Prop};
use motion_format::shorthand::{Axis, Behaviour};
use motion_format::{Content, Curve, Layer, Scene};

use tracing::{debug_span, field};

use crate::graph::{Node, NodeKind, SceneGraph};
use crate::text::TextMeasure;
use crate::track::{Key, Track};

/// Compiles `scene` into a [`SceneGraph`], using `measure` to size text.
pub fn compile(scene: &Scene, measure: &dyn TextMeasure) -> SceneGraph {
    // Only animated tracks count: every node has one track per property.
    let span = debug_span!("compile", nodes = scene.layers.len(), tracks = field::Empty);
    let _entered = span.enter();
    let width = f64::from(scene.size.width);
    let height = f64::from(scene.size.height);
    let nodes: Vec<Node> = scene
        .layers
        .iter()
        .map(|layer| node(layer, scene, (width, height), measure))
        .collect();
    // Counting walks every track, so skip it when nobody listens.
    if !span.is_disabled() {
        let animated = nodes
            .iter()
            .flat_map(|node| &node.tracks)
            .filter(|track| !track.is_constant())
            .count();
        span.record("tracks", animated);
    }
    SceneGraph {
        width,
        height,
        fps: scene.fps,
        duration_frames: scene.duration.to_frames(scene.fps),
        nodes,
    }
}

/// Compiles one layer into a node.
fn node(layer: &Layer, scene: &Scene, canvas: (f64, f64), measure: &dyn TextMeasure) -> Node {
    let (kind, width, height) = match &layer.content {
        Content::Text { text, font } => {
            let (w, h) = measure.measure(text, font);
            (NodeKind::Text, w, h)
        }
        Content::Rect(r) => (NodeKind::Rect, r.width, r.height),
    };
    let entrance = entrance(layer, scene);
    Node {
        id: layer.id.clone(),
        kind,
        width,
        height,
        tracks: tracks(layer, entrance, canvas, scene.fps),
        follows_at: (
            follows_at(layer, entrance, Axis::X),
            follows_at(layer, entrance, Axis::Y),
        ),
    }
}

/// The layer's entrance and its preset, which parsing has checked exists.
fn entrance<'s>(layer: &'s Layer, scene: &'s Scene) -> Option<(&'s Behaviour, &'s PresetSpec)> {
    let behaviour = layer.enter.as_ref()?;
    scene
        .preset(&behaviour.preset)
        .map(|spec| (behaviour, spec))
}

/// The tracks of `layer`, parallel to [`PROPERTIES`]: rest, then entrance, then `anim`.
fn tracks(
    layer: &Layer,
    entrance: Option<(&Behaviour, &PresetSpec)>,
    canvas: (f64, f64),
    fps: f64,
) -> Vec<Track> {
    let rest: Vec<f64> = PROPERTIES
        .iter()
        .map(|spec| spec.rest(&layer.at, canvas))
        .collect();
    let mut tracks: Vec<Track> = rest.iter().map(|&v| Track::constant(v)).collect();
    if let Some((behaviour, spec)) = entrance {
        for &(prop, from) in &spec.props {
            tracks[prop.index()] = entrance_track(behaviour, from, rest[prop.index()], fps);
        }
    }
    // Parsing forbids `anim` on an entrance property; this wins only in code-built scenes.
    for track in &layer.anim {
        tracks[track.property.index()] = anim_track(track, fps);
    }
    tracks
}

/// One entrance property: from its start to `rest` over the behaviour's frames.
fn entrance_track(behaviour: &Behaviour, from: Start, rest: f64, fps: f64) -> Track {
    let (start, end) = behaviour.frames(fps);
    Track::new(vec![
        Key {
            frame: start,
            value: from.resolve(rest, behaviour.distance),
            curve: behaviour.curve,
        },
        Key {
            frame: end,
            value: rest,
            curve: Curve::Linear,
        },
    ])
}

/// An `anim` track with its times in frames.
fn anim_track(track: &AnimTrack, fps: f64) -> Track {
    let keys = track
        .keys
        .iter()
        .map(|k| Key {
            frame: k.time.to_frames(fps),
            value: k.value,
            curve: k.curve,
        })
        .collect();
    Track::new(keys)
}

/// Whether moving `at` moves the whole motion on `axis`: not when `anim` drives
/// it, nor when the entrance starts it at an absolute value.
fn follows_at(layer: &Layer, entrance: Option<(&Behaviour, &PresetSpec)>, axis: Axis) -> bool {
    let on_axis = |prop: Prop| prop.spec().rest_from_at == Some(axis);
    let by_anim = layer.anim.iter().any(|track| on_axis(track.property));
    let by_start = entrance.is_some_and(|(_, spec)| {
        spec.props
            .iter()
            .any(|&(prop, start)| on_axis(prop) && matches!(start, Start::Value(_)))
    });
    !by_anim && !by_start
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
