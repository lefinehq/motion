//! Step 2 (Compile): resolves positions, sets text, links parents and
//! turns each layer's resting values, entrance and `anim` into one track per
//! property ([`PROPERTIES`](motion_format::property::PROPERTIES)); each shape
//! item and effect likewise gets a track per row of its kind's registry, and
//! each composition its own nodes on its own clock.

use std::sync::Arc;

use kurbo::Rect;
use motion_format::anim::{AnimTrack, Easing};
use motion_format::key::layer_path;
use motion_format::preset::{PresetSpec, Start};
use motion_format::property::{Prop, Rest, Target};
use motion_format::shorthand::{Axis, Behaviour, MatteMode};
use motion_format::units::Color;
use motion_format::{Content, Item, Layer, Result, Scene, Value, ValueType};
use motion_text::Typesetter;

use tracing::{debug_span, field};

use crate::graph::{CompGraph, ItemNode, Node, NodeContent, PropertyTrack, SceneGraph, Timing};
use crate::text;
use crate::track::{KeySpec, Track};

/// Compiles `scene` into a [`SceneGraph`], setting its text with `typesetter`.
///
/// # Errors
/// A [`Diagnostic`](motion_format::Diagnostic) when a text layer names a font `typesetter` lacks or
/// holds a character its font has no glyph for.
///
/// # Panics
/// If following parents leads back to a layer, or a `comp` layer names a
/// composition the scene lacks, which validation rules out.
pub fn compile(scene: &Scene, typesetter: &mut Typesetter) -> Result<SceneGraph> {
    // Only animated tracks count: every node has one track per property.
    let span = debug_span!("compile", nodes = scene.layers.len(), tracks = field::Empty);
    let _entered = span.enter();
    let width = f64::from(scene.size.width);
    let height = f64::from(scene.size.height);
    let comps = scene
        .comps
        .iter()
        .map(|comp| {
            let canvas = Rect::new(
                0.0,
                0.0,
                f64::from(comp.size.width),
                f64::from(comp.size.height),
            );
            Ok(CompGraph {
                id: comp.id.clone(),
                width: canvas.width(),
                height: canvas.height(),
                fps: comp.fps,
                duration_frames: comp.duration.to_frames(comp.fps),
                background: rgba(comp.background),
                nodes: nodes(&comp.layers, canvas, comp.fps, scene, typesetter)
                    .map_err(|e| e.under(motion_format::key::comp_path(&comp.id)))?,
            })
        })
        .collect::<Result<Vec<CompGraph>>>()?;
    let canvas = Rect::new(0.0, 0.0, width, height);
    let nodes = nodes(&scene.layers, canvas, scene.fps, scene, typesetter)?;
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
    Ok(SceneGraph {
        width,
        height,
        fps: scene.fps,
        duration_frames: scene.duration.to_frames(scene.fps),
        nodes,
        comps,
    })
}

/// The nodes of one `canvas` whose clock runs at `fps`, with paths relative
/// to the list's owner.
fn nodes(
    layers: &[Layer],
    canvas: Rect,
    fps: f64,
    scene: &Scene,
    typesetter: &mut Typesetter,
) -> Result<Vec<Node>> {
    let contents = layers
        .iter()
        .enumerate()
        .map(|(i, layer)| {
            node_content(layer, scene, fps, typesetter).map_err(|e| e.under(layer_path(i)))
        })
        .collect::<Result<Vec<_>>>()?;
    let boxes: Vec<Rect> = contents.iter().map(|c| content_box(c, scene)).collect();
    let parents: Vec<Option<usize>> = layers
        .iter()
        .map(|layer| {
            let id = layer.parent.as_ref()?;
            layers.iter().position(|l| &l.id == id)
        })
        .collect();
    assert_no_cycle(&parents);
    // Validation has checked that each matte names a layer of this list.
    let mattes: Vec<Option<(usize, MatteMode)>> = layers
        .iter()
        .map(|layer| {
            let matte = layer.matte.as_ref()?;
            let index = layers.iter().position(|l| l.id == matte.layer)?;
            Some((index, matte.mode))
        })
        .collect();
    Ok(layers
        .iter()
        .zip(contents)
        .enumerate()
        .map(|(i, (layer, content))| {
            // `at` resolves in the parent's box, or on the canvas without a parent.
            let extent = parents[i].map_or(canvas, |p| boxes[p]);
            Node {
                matte: mattes[i],
                ..node(layer, scene, fps, content, boxes[i], parents[i], extent)
            }
        })
        .collect())
}

/// A node's box in its own coordinates: a text layout's bounds, a rect's or
/// a composition's canvas from the origin, an image's `box`, and empty at
/// the origin for shapes and pivots.
fn content_box(content: &NodeContent, scene: &Scene) -> Rect {
    match content {
        NodeContent::Text { text, .. } => text.layout.bounds(),
        NodeContent::Rect(r) => Rect::new(0.0, 0.0, r.width, r.height),
        NodeContent::Comp(index) => {
            let size = scene.comps[*index].size;
            Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height))
        }
        NodeContent::Image(image) => image.frame,
        NodeContent::Shapes(_) | NodeContent::Pivot => Rect::ZERO,
    }
}

/// What a layer draws, compiled; text is set here.
fn node_content(
    layer: &Layer,
    scene: &Scene,
    fps: f64,
    typesetter: &mut Typesetter,
) -> Result<NodeContent> {
    Ok(match &layer.content {
        Content::Text(content) => NodeContent::Text {
            text: text::set(content, typesetter)?,
            animators: item_nodes(&content.animators, fps),
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
        Content::Image(image) => NodeContent::Image(Arc::new(image.clone())),
        Content::Pivot => NodeContent::Pivot,
    })
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
        written: item
            .kind
            .registry
            .props()
            .filter(|&prop| {
                item.value(prop).is_some() || item.anim.iter().any(|t| t.target.prop == prop)
            })
            .collect(),
    }
}

/// An 8-bit colour as straight RGBA from 0 to 1.
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
    content: NodeContent,
    bounds: Rect,
    parent: Option<usize>,
    extent: Rect,
) -> Node {
    let entrance = entrance(layer, scene);
    Node {
        id: layer.id.clone(),
        content,
        fill: layer.fill,
        bounds,
        parent,
        timing: Timing {
            span: layer
                .span
                .map(|span| (span.start.to_frames(fps), span.end.to_frames(fps))),
            offset: layer.offset.to_frames(fps),
            stretch: layer.stretch,
        },
        tracks: tracks(layer, entrance, extent, bounds, fps),
        follows_at: (
            follows_at(layer, entrance, Axis::X),
            follows_at(layer, entrance, Axis::Y),
        ),
        fx: item_nodes(&layer.fx, fps),
        hidden: layer.hidden,
        matte: None,
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
    extent: Rect,
    own: Rect,
    fps: f64,
) -> Vec<PropertyTrack> {
    let mut written: Vec<Written> = Vec::new();
    if let Some((behaviour, spec)) = entrance {
        for &(target, from) in &spec.props {
            let rest = target.rest_of(layer.rest(target.prop, extent, own));
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
            let rest = layer.rest(prop, extent, own);
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
