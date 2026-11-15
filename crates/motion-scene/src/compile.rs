//! Step 2 (Compile): resolves positions, sets text, links parents and
//! turns each layer's resting values, entrance and `anim` into one track per
//! property ([`PROPERTIES`](motion_format::property::PROPERTIES)); each shape
//! item and effect likewise gets a track per row of its kind's registry, and
//! each composition its own nodes on its own clock.

use std::sync::Arc;

use kurbo::Rect;
use motion_format::Place;
use motion_format::anim::{AnimTrack, Easing};
use motion_format::preset::{PresetSpec, Start, parts_animator};
use motion_format::property::{Prop, Rest, Target};
use motion_format::shorthand::{Axis, Behaviour, MatteMode};
use motion_format::units::Color;
use motion_format::{Comp, Content, Item, Layer, Result, Scene, Value, ValueType};
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
    let span = debug_span!(
        "compile",
        nodes = scene.root.layers.len(),
        tracks = field::Empty
    );
    let _entered = span.enter();
    let mut build = Build {
        scene,
        typesetter,
        instances: Vec::new(),
    };
    let mut comps = scene
        .comps
        .iter()
        .map(|comp| {
            build
                .comp_graph(comp, &comp.layers)
                .map_err(|e| e.under(motion_format::key::comp_path(&comp.id)))
        })
        .collect::<Result<Vec<CompGraph>>>()?;
    let root = build.comp_graph(&scene.root, &scene.root.layers)?;
    comps.append(&mut build.instances);
    // Counting walks every track, so skip it when nobody listens.
    if !span.is_disabled() {
        let animated = std::iter::once(&root)
            .chain(&comps)
            .flat_map(|c| &c.nodes)
            .flat_map(|node| &node.tracks)
            .filter(|track| !track.is_constant())
            .count();
        span.record("tracks", animated);
    }
    Ok(SceneGraph { root, comps })
}

/// What compiling reads and what it adds on the way: the scene, the
/// typesetter that sets its text, and the compositions of `comp` layers
/// that override theirs, which follow the scene's own in the graph.
struct Build<'a> {
    scene: &'a Scene,
    typesetter: &'a mut Typesetter,
    /// One composition per `comp` layer with overrides, in the order met;
    /// the first is at index `scene.comps.len()` of the graph's comps.
    instances: Vec<CompGraph>,
}

impl Build<'_> {
    /// One composition, the scene's root, one under `comps` or one a `comp`
    /// layer overrides, drawn with `layers`, with paths relative to it.
    fn comp_graph(&mut self, comp: &Comp, layers: &[Layer]) -> Result<CompGraph> {
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
            nodes: self.nodes(layers, canvas, comp.fps)?,
        })
    }

    /// The nodes of one `canvas` whose clock runs at `fps`, with paths relative
    /// to the list's owner.
    fn nodes(&mut self, layers: &[Layer], canvas: Rect, fps: f64) -> Result<Vec<Node>> {
        let contents = layers
            .iter()
            .map(|layer| {
                self.node_content(layer, fps)
                    .map_err(|e| e.under(Place::of_layer(&layer.id)))
            })
            .collect::<Result<Vec<_>>>()?;
        let boxes: Vec<Rect> = contents.iter().map(|c| self.content_box(c)).collect();
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
        // Validation has checked that each input names a layer of this list.
        let inputs: Vec<Vec<Option<usize>>> = layers
            .iter()
            .map(|layer| {
                layer
                    .fx
                    .iter()
                    .filter(|item| !item.hidden)
                    .map(|item| {
                        let id = item.input.as_deref()?;
                        layers.iter().position(|l| l.id == id)
                    })
                    .collect()
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
                    inputs: inputs[i].clone(),
                    ..node(
                        layer, self.scene, fps, content, boxes[i], parents[i], extent,
                    )
                }
            })
            .collect())
    }

    /// A node's box in its own coordinates: a text layout's bounds, a rect's or
    /// a composition's canvas from the origin, an image's `box`, and empty at
    /// the origin for shapes and pivots.
    fn content_box(&self, content: &NodeContent) -> Rect {
        match content {
            NodeContent::Text { text, .. } => text.layout.bounds(),
            NodeContent::Rect(r) => Rect::new(0.0, 0.0, r.width, r.height),
            NodeContent::Comp(index) => {
                let shared = self.scene.comps.len();
                let (width, height) = match self.scene.comps.get(*index) {
                    Some(comp) => (f64::from(comp.size.width), f64::from(comp.size.height)),
                    None => {
                        let instance = &self.instances[*index - shared];
                        (instance.width, instance.height)
                    }
                };
                Rect::new(0.0, 0.0, width, height)
            }
            NodeContent::Image(image) => image.frame,
            NodeContent::Video(video) => video.frame,
            NodeContent::Shapes(_) | NodeContent::Pivot | NodeContent::Filter => Rect::ZERO,
        }
    }

    /// What a layer draws, compiled; text is set here.
    fn node_content(&mut self, layer: &Layer, fps: f64) -> Result<NodeContent> {
        Ok(match &layer.content {
            Content::Text(content) => NodeContent::Text {
                text: text::set(content, self.typesetter)?,
                animators: entrance(layer, self.scene)
                    .and_then(|(behaviour, spec)| parts_animator(spec, behaviour, fps))
                    .iter()
                    .chain(&content.animators)
                    .filter(|item| !item.hidden)
                    .map(|item| item_node(item, fps))
                    .collect(),
            },
            Content::Rect(r) => NodeContent::Rect(*r),
            Content::Shapes(items) => NodeContent::Shapes(item_nodes(items, fps)),
            Content::Comp(comp) => {
                let shared = self
                    .scene
                    .comps
                    .iter()
                    .position(|c| c.id == comp.id)
                    .expect("validation checks every comp exists");
                match &comp.instance {
                    None => NodeContent::Comp(shared),
                    Some(layers) => {
                        let graph = self.comp_graph(&self.scene.comps[shared], layers)?;
                        self.instances.push(graph);
                        NodeContent::Comp(self.scene.comps.len() + self.instances.len() - 1)
                    }
                }
            }
            Content::Image(image) => NodeContent::Image(Arc::new(image.clone())),
            Content::Video(video) => NodeContent::Video(Arc::new(video.clone())),
            Content::Pivot => NodeContent::Pivot,
            Content::Filter => NodeContent::Filter,
        })
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
            keys: anim_keys(
                track,
                &track.target.rest_of(item.rest(track.target.prop)),
                fps,
            ),
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
    // An entrance on the parts of a text is one of its animators instead.
    let entrance = entrance(layer, scene).filter(|(behaviour, _)| behaviour.parts.is_none());
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
        mask: item_nodes(&layer.mask, fps),
        fx: item_nodes(&layer.fx, fps),
        hidden: layer.hidden,
        adjust: layer.adjust,
        collapse: layer.collapse,
        matte: None,
        blend: layer.blend,
        inputs: Vec::new(),
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
        let rest = track
            .target
            .rest_of(layer.rest(track.target.prop, extent, own));
        written.push(Written {
            target: track.target,
            keys: anim_keys(track, &rest, fps),
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

/// An `anim` track's keys with their times in frames, and values written
/// against the rest resolved against `rest`, the target's resting value.
fn anim_keys(track: &AnimTrack, rest: &Value, fps: f64) -> Vec<KeySpec> {
    track
        .keys
        .iter()
        .map(|k| KeySpec {
            frame: k.time.to_frames(fps),
            value: k.value.resolve(rest),
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
