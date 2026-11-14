//! Step 2 (Compile): resolves positions, measures text, links parents and
//! turns each layer's resting values, entrance and `anim` into one track per
//! property ([`PROPERTIES`](motion_format::property::PROPERTIES)).

use motion_format::anim::{AnimTrack, Easing};
use motion_format::preset::{PresetSpec, Start};
use motion_format::property::{Prop, Target};
use motion_format::shorthand::{Axis, Behaviour};
use motion_format::{Content, Layer, Scene, Value, ValueType};

use tracing::{debug_span, field};

use crate::graph::{Node, NodeKind, PropertyTrack, SceneGraph, Timing};
use crate::text::TextMeasure;
use crate::track::{KeySpec, Track};

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
    let fps = scene.fps;
    Node {
        id: layer.id.clone(),
        kind,
        width,
        height,
        parent: layer
            .parent
            .as_ref()
            .and_then(|id| scene.layers.iter().position(|l| &l.id == id)),
        timing: Timing {
            span: layer
                .span
                .map(|span| (span.start.to_frames(fps), span.end.to_frames(fps))),
            offset: layer.offset.to_frames(fps),
            stretch: layer.stretch,
        },
        tracks: tracks(layer, entrance, canvas, (width, height), fps),
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

/// The keys one writer gives one target.
struct Written {
    target: Target,
    keys: Vec<KeySpec>,
}

/// The tracks of `layer`, one per property in registry order: rest, then entrance, then `anim`.
fn tracks(
    layer: &Layer,
    entrance: Option<(&Behaviour, &PresetSpec)>,
    canvas: (f64, f64),
    size: (f64, f64),
    fps: f64,
) -> Vec<PropertyTrack> {
    let mut written: Vec<Written> = Vec::new();
    if let Some((behaviour, spec)) = entrance {
        for &(target, from) in &spec.props {
            let rest = target.rest_of(layer.rest(target.prop, canvas, size));
            written.push(Written {
                target,
                keys: entrance_keys(behaviour, from, rest, fps),
            });
        }
    }
    // Parsing forbids `anim` on an entrance target; this wins only in code-built scenes.
    for track in &layer.anim {
        written.retain(|w| !w.target.overlaps(track.target));
        written.push(Written {
            target: track.target,
            keys: anim_keys(track, fps),
        });
    }
    Prop::all()
        .map(|prop| {
            let rest = layer.rest(prop, canvas, size);
            property_track(prop, rest, &written, fps)
        })
        .collect()
}

/// One property's track: whole when nothing animates a single component of it,
/// else one number track per component.
fn property_track(prop: Prop, rest: Value, written: &[Written], fps: f64) -> PropertyTrack {
    let spec = prop.spec();
    let mine: Vec<&Written> = written.iter().filter(|w| w.target.prop == prop).collect();
    let whole = mine.iter().find(|w| w.target.component.is_none());
    if mine.iter().all(|w| w.target.component.is_none()) {
        return PropertyTrack::Whole(match whole {
            Some(w) => Track::new(spec.ty, w.keys.clone(), fps, spec.spatial),
            None => Track::constant(rest),
        });
    }
    let components = (0..spec.ty.dims()).map(|i| {
        let keys = match mine.iter().find(|w| w.target.component == Some(i)) {
            Some(w) => w.keys.clone(),
            None => match whole {
                Some(w) => w.keys.iter().map(|k| component_key(k, i)).collect(),
                None => vec![KeySpec::linear(0.0, Value::Number(rest.components()[i]))],
            },
        };
        Track::new(ValueType::Number, keys, fps, false)
    });
    PropertyTrack::Split(components.collect())
}

/// Component `i` of a key of a whole value.
fn component_key(key: &KeySpec, i: usize) -> KeySpec {
    KeySpec {
        frame: key.frame,
        value: Value::Number(key.value.components()[i]),
        easing: Easing::All(key.easing.of(i)),
        path: None,
    }
}

/// One entrance target: from its start to `rest` over the behaviour's frames.
fn entrance_keys(behaviour: &Behaviour, from: Start, rest: Value, fps: f64) -> Vec<KeySpec> {
    let (start, end) = behaviour.frames(fps);
    vec![
        KeySpec {
            frame: start,
            value: from.resolve(rest, behaviour.distance),
            easing: Easing::All(behaviour.curve),
            path: None,
        },
        KeySpec::linear(end, rest),
    ]
}

/// An `anim` track's keys with their times in frames.
fn anim_keys(track: &AnimTrack, fps: f64) -> Vec<KeySpec> {
    track
        .keys
        .iter()
        .map(|k| KeySpec {
            frame: k.time.to_frames(fps),
            value: k.value,
            easing: k.easing.clone(),
            path: k.path,
        })
        .collect()
}

/// Whether moving `at` moves the whole motion on `axis`: not when `anim` drives
/// it, nor when the entrance starts it at an absolute value.
fn follows_at(layer: &Layer, entrance: Option<(&Behaviour, &PresetSpec)>, axis: Axis) -> bool {
    let on_axis = |target: Target| {
        target.prop == Prop::POSITION && target.component.is_none_or(|i| i == axis_index(axis))
    };
    let by_anim = layer.anim.iter().any(|track| on_axis(track.target));
    let by_start = entrance.is_some_and(|(_, spec)| {
        spec.props
            .iter()
            .any(|&(target, start)| on_axis(target) && matches!(start, Start::Value(_)))
    });
    !by_anim && !by_start
}

/// The component of `position` that `axis` names.
fn axis_index(axis: Axis) -> usize {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
    }
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
