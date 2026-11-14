//! Step 2 (Compile): resolves positions, measures text, links parents and
//! turns each layer's resting values, entrance and `anim` into one track per
//! property ([`PROPERTIES`](motion_format::property::PROPERTIES)); each shape
//! item and effect likewise gets a track per row of its kind's registry, and
//! each composition its own nodes on its own clock.

use motion_format::anim::{AnimTrack, Easing};
use motion_format::preset::{PresetSpec, Start};
use motion_format::property::{Prop, Rest, Target};
use motion_format::shorthand::{Axis, Behaviour};
use motion_format::units::Color;
use motion_format::{Content, Item, Layer, Scene, Value, ValueType};

use tracing::{debug_span, field};

use crate::graph::{CompGraph, ItemNode, Node, NodeContent, PropertyTrack, SceneGraph, Timing};
use crate::text::TextMeasure;
use crate::track::{KeySpec, Track};

/// Compiles `scene` into a [`SceneGraph`], using `measure` to size text.
///
/// # Panics
/// If following parents leads back to a layer, or a `comp` layer names a
/// composition the scene lacks, which validation rules out.
pub fn compile(scene: &Scene, measure: &dyn TextMeasure) -> SceneGraph {
    // Only animated tracks count: every node has one track per property.
    let span = debug_span!("compile", nodes = scene.layers.len(), tracks = field::Empty);
    let _entered = span.enter();
    let width = f64::from(scene.size.width);
    let height = f64::from(scene.size.height);
    let comps: Vec<CompGraph> = scene
        .comps
        .iter()
        .map(|comp| {
            let canvas = (f64::from(comp.size.width), f64::from(comp.size.height));
            CompGraph {
                id: comp.id.clone(),
                width: canvas.0,
                height: canvas.1,
                fps: comp.fps,
                duration_frames: comp.duration.to_frames(comp.fps),
                background: rgba(comp.background),
                nodes: nodes(&comp.layers, canvas, comp.fps, scene, measure),
            }
        })
        .collect();
    let nodes = nodes(&scene.layers, (width, height), scene.fps, scene, measure);
    // Counting walks every track, so skip it when nobody listens.
    if !span.is_disabled() {
        let animated = nodes
            .iter()
            .chain(comps.iter().flat_map(|c| &c.nodes))
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
        comps,
    }
}

/// The nodes of one canvas of `canvas` pixels whose clock runs at `fps`.
fn nodes(
    layers: &[Layer],
    canvas: (f64, f64),
    fps: f64,
    scene: &Scene,
    measure: &dyn TextMeasure,
) -> Vec<Node> {
    let boxes: Vec<(f64, f64)> = layers
        .iter()
        .map(|layer| content_box(layer, scene, measure))
        .collect();
    let parents: Vec<Option<usize>> = layers
        .iter()
        .map(|layer| {
            let id = layer.parent.as_ref()?;
            layers.iter().position(|l| &l.id == id)
        })
        .collect();
    assert_no_cycle(&parents);
    layers
        .iter()
        .enumerate()
        .map(|(i, layer)| {
            // `at` resolves in the parent's box, or on the canvas without a parent.
            let extent = parents[i].map_or(canvas, |p| boxes[p]);
            node(layer, scene, fps, boxes[i], parents[i], extent)
        })
        .collect()
}

/// A layer's box in pixels: measured text, a rect's size, a composition's
/// canvas, and none for shapes and pivots, whose coordinates start at their anchor.
fn content_box(layer: &Layer, scene: &Scene, measure: &dyn TextMeasure) -> (f64, f64) {
    match &layer.content {
        Content::Text { text, font } => measure.measure(text, font),
        Content::Rect(r) => (r.width, r.height),
        Content::Comp(id) => {
            let comp = scene.comp(id).expect("validation checks every comp exists");
            (f64::from(comp.size.width), f64::from(comp.size.height))
        }
        Content::Shapes(_) | Content::Pivot => (0.0, 0.0),
    }
}

/// What a layer draws, compiled.
fn node_content(layer: &Layer, scene: &Scene, fps: f64) -> NodeContent {
    match &layer.content {
        Content::Text { text, font } => NodeContent::Text {
            text: text.clone(),
            font: font.clone(),
        },
        Content::Rect(r) => NodeContent::Rect(*r),
        Content::Shapes(items) => NodeContent::Shapes(item_nodes(items, fps)),
        Content::Comp(id) => NodeContent::Comp(
            scene
                .comps
                .iter()
                .position(|c| &c.id == id)
                .expect("validation checks every comp exists"),
        ),
        Content::Pivot => NodeContent::Pivot,
    }
}

/// The items that are not hidden, compiled.
fn item_nodes(items: &[Item], fps: f64) -> Vec<ItemNode> {
    items
        .iter()
        .filter(|item| !item.hidden)
        .map(|item| item_node(item, fps))
        .collect()
}

/// One item: a track per row of its kind's registry, from its rest and `anim`.
fn item_node(item: &Item, fps: f64) -> ItemNode {
    let written: Vec<Written> = item
        .anim
        .iter()
        .map(|track| Written {
            target: track.target,
            keys: anim_keys(track, fps),
        })
        .collect();
    ItemNode {
        kind: item.kind,
        id: item.id.clone(),
        tracks: item
            .kind
            .registry
            .props()
            .map(|prop| property_track(prop, item.rest(prop), &written, fps))
            .collect(),
        settings: item.settings.clone(),
        items: item_nodes(&item.items, fps),
    }
}

/// A colour as straight RGBA from 0 to 1.
fn rgba(color: Color) -> [f64; 4] {
    let value = Value::from(color);
    let c = value.components();
    [c[0], c[1], c[2], c[3]]
}

/// Panics on a parent cycle, which validation rules out; a scene built in
/// code can still hold one, and evaluation would never end.
fn assert_no_cycle(parents: &[Option<usize>]) {
    for start in 0..parents.len() {
        let mut at = parents[start];
        for _ in 0..parents.len() {
            match at {
                Some(p) => {
                    assert_ne!(p, start, "layer {start} is its own ancestor");
                    at = parents[p];
                }
                None => break,
            }
        }
    }
}

/// Compiles one layer into a node, with its content box, its parent's index,
/// and the extent `at` resolves in.
fn node(
    layer: &Layer,
    scene: &Scene,
    fps: f64,
    (width, height): (f64, f64),
    parent: Option<usize>,
    extent: (f64, f64),
) -> Node {
    let entrance = entrance(layer, scene);
    Node {
        id: layer.id.clone(),
        content: node_content(layer, scene, fps),
        fill: rgba(layer.fill),
        width,
        height,
        parent,
        timing: Timing {
            span: layer
                .span
                .map(|span| (span.start.to_frames(fps), span.end.to_frames(fps))),
            offset: layer.offset.to_frames(fps),
            stretch: layer.stretch,
        },
        tracks: tracks(layer, entrance, extent, (width, height), fps),
        follows_at: (
            follows_at(layer, entrance, Axis::X),
            follows_at(layer, entrance, Axis::Y),
        ),
        fx: item_nodes(&layer.fx, fps),
        hidden: layer.hidden,
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
    extent: (f64, f64),
    size: (f64, f64),
    fps: f64,
) -> Vec<PropertyTrack> {
    let mut written: Vec<Written> = Vec::new();
    if let Some((behaviour, spec)) = entrance {
        for &(target, from) in &spec.props {
            let rest = target.rest_of(layer.rest(target.prop, extent, size));
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
            let rest = layer.rest(prop, extent, size);
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
    let components = (0..spec.components.len()).map(|i| {
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
            value: from.resolve(&rest, behaviour.distance),
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
            value: k.value.clone(),
            easing: k.easing.clone(),
            path: k.path,
        })
        .collect()
}

/// Whether moving `at` moves the whole motion on `axis`: not when `anim` drives
/// it, nor when the entrance starts it at an absolute value.
fn follows_at(layer: &Layer, entrance: Option<(&Behaviour, &PresetSpec)>, axis: Axis) -> bool {
    let on_axis = |target: Target| {
        target.prop.spec().rest == Rest::At && target.component.is_none_or(|i| i == axis.index())
    };
    let by_anim = layer.anim.iter().any(|track| on_axis(track.target));
    let by_start = entrance.is_some_and(|(_, spec)| {
        spec.props
            .iter()
            .any(|&(target, start)| on_axis(target) && matches!(start, Start::Value(_)))
    });
    !by_anim && !by_start
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
