//! Step 2 (Compile): resolves positions, sets text, links parents and
//! turns each layer's resting values, entrance and `anim` into one track per
//! property ([`PROPERTIES`](nakkash_format::property::PROPERTIES)); each shape
//! item and effect likewise gets a track per row of its kind's registry, and
//! each composition its own nodes on its own clock.

use std::sync::Arc;

use kurbo::Rect;
use nakkash_format::Place;
use nakkash_format::anim::{AnimTrack, Easing};
use nakkash_format::preset::{PresetSpec, Start, parts_animator};
use nakkash_format::property::{Prop, Rest, Target};
use nakkash_format::shorthand::{Axis, Behaviour, MatteMode};
use nakkash_format::units::Color;
use nakkash_format::{Comp, Content, Item, Layer, Result, Scene, Value, ValueType};
use nakkash_text::Typesetter;

use tracing::{debug_span, field};

use crate::graph::{CompGraph, ItemNode, Node, NodeContent, PropertyTrack, SceneGraph, Timing};
use crate::level::{Ear, Levels};
use crate::text;
use crate::track::{KeySpec, Track};

/// Compiles `scene` into a [`SceneGraph`], setting its text with `typesetter`.
///
/// # Errors
/// A [`Diagnostic`](nakkash_format::Diagnostic) when a text layer names a font `typesetter` lacks or
/// holds a character its font has no glyph for.
///
/// # Panics
/// If following parents leads back to a layer, or a `comp` layer names a
/// composition the scene lacks, which validation rules out.
pub fn compile(scene: &Scene, typesetter: &mut Typesetter) -> Result<SceneGraph> {
    compile_with(scene, typesetter, &Levels::none())
}

/// Compiles `scene` as [`compile`] does, with the `levels` its listening
/// tracks measured: each becomes a key every frame of its list's clock
/// ([`Levels`]). A track without levels holds its value at silence.
///
/// # Errors
/// As [`compile`].
///
/// # Panics
/// As [`compile`].
pub fn compile_with(
    scene: &Scene,
    typesetter: &mut Typesetter,
    levels: &Levels,
) -> Result<SceneGraph> {
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
        levels,
        instances: Vec::new(),
    };
    let mut comps = scene
        .comps
        .iter()
        .map(|comp| {
            build
                .comp_graph(comp, &comp.layers)
                .map_err(|e| e.under(nakkash_format::key::comp_path(&comp.id)))
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
    /// What the listening tracks measured.
    levels: &'a Levels,
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
            shutter: comp.shutter,
            nodes: self.nodes(
                layers,
                (canvas, comp.duration.to_frames(comp.fps)),
                comp.fps,
                &comp.id,
            )?,
        })
    }

    /// The nodes of one `canvas` whose clock runs at `fps` and ends at frame
    /// `length`, the list of the composition `comp`, with paths relative to
    /// the list's owner.
    fn nodes(
        &mut self,
        layers: &[Layer],
        (canvas, length): (Rect, f64),
        fps: f64,
        comp: &str,
    ) -> Result<Vec<Node>> {
        let size = (canvas.width(), canvas.height());
        let levels = self.levels;
        let ear = |layer: &Layer| Ear {
            levels,
            comp,
            fps,
            timing: timing(layer, fps),
        };
        let contents = layers
            .iter()
            .map(|layer| {
                self.node_content(layer, (fps, size), ear(layer))
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
        // Validation has checked that each outline names a mask of its own
        // layer; a hidden mask is left out, so nothing follows it.
        let outlines: Vec<Vec<Option<usize>>> = layers
            .iter()
            .map(|layer| {
                let shown: Vec<&Item> = layer.mask.iter().filter(|m| !m.hidden).collect();
                layer
                    .fx
                    .iter()
                    .filter(|item| !item.hidden)
                    .map(|item| {
                        let id = item.outline.as_deref()?;
                        shown.iter().position(|m| m.id.as_deref() == Some(id))
                    })
                    .collect()
            })
            .collect();
        // Validation has checked that each pivot names another layer of
        // this list.
        let pivots: Vec<Vec<Option<[usize; 4]>>> = layers
            .iter()
            .map(|layer| {
                layer
                    .fx
                    .iter()
                    .filter(|item| !item.hidden)
                    .map(|item| {
                        let ids = item.pivots.as_ref()?.ids();
                        let index = |id: &String| layers.iter().position(|l| &l.id == id);
                        Some([
                            index(&ids[0])?,
                            index(&ids[1])?,
                            index(&ids[2])?,
                            index(&ids[3])?,
                        ])
                    })
                    .collect()
            })
            .collect();
        // A pivot draws nothing, so its motion blur blurs every layer it
        // carries, at any depth ([D57]).
        //
        // [D57]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter
        let blurs_what_it_carries =
            |p: usize| layers[p].mblur && matches!(contents[p], NodeContent::Pivot);
        let mblur: Vec<bool> = (0..layers.len())
            .map(|i| {
                layers[i].mblur
                    || std::iter::successors(parents[i], |&p| parents[p]).any(blurs_what_it_carries)
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
                    mblur: mblur[i],
                    matte: mattes[i],
                    inputs: inputs[i].clone(),
                    outlines: outlines[i].clone(),
                    pivots: pivots[i].clone(),
                    ..node(
                        layer,
                        self.scene,
                        (fps, length, size),
                        content,
                        (boxes[i], parents[i], extent),
                        ear(layer),
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
            NodeContent::Shapes(_)
            | NodeContent::Audio(_)
            | NodeContent::Pivot
            | NodeContent::Camera { .. }
            | NodeContent::Filter => Rect::ZERO,
        }
    }

    /// What a layer draws, compiled; text is set here.
    fn node_content(
        &mut self,
        layer: &Layer,
        (fps, canvas): (f64, (f64, f64)),
        ear: Ear<'_>,
    ) -> Result<NodeContent> {
        Ok(match &layer.content {
            Content::Text(content) => {
                let (text, keys) = text::set_keyed(content, self.typesetter, fps)?;
                NodeContent::Text {
                    text,
                    keys,
                    animators: entrance(layer, self.scene)
                        .and_then(|(behaviour, spec)| parts_animator(spec, behaviour, fps, canvas))
                        .iter()
                        .chain(&content.animators)
                        .filter(|item| !item.hidden)
                        .map(|item| item_node(item, fps, ear))
                        .collect(),
                }
            }
            Content::Rect(r) => NodeContent::Rect(*r),
            Content::Shapes(items) => NodeContent::Shapes(item_nodes(items, fps, ear)),
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
            Content::Audio(audio) => NodeContent::Audio(Arc::new(audio.clone())),
            Content::Pivot => NodeContent::Pivot,
            Content::Camera(camera) => NodeContent::Camera { aim: camera.aim },
            Content::Filter => NodeContent::Filter,
        })
    }
}

/// The items that are not hidden, compiled.
fn item_nodes(items: &[Item], fps: f64, ear: Ear<'_>) -> Vec<ItemNode> {
    items
        .iter()
        .filter(|item| !item.hidden)
        .map(|item| item_node(item, fps, ear))
        .collect()
}

/// One item: a track per row of its kind's registry, from its rest and `anim`.
fn item_node(item: &Item, fps: f64, ear: Ear<'_>) -> ItemNode {
    let written: Vec<Written> = item
        .anim
        .iter()
        .map(|track| Written {
            target: track.target,
            keys: anim_keys(
                track,
                &track.target.rest_of(item.rest(track.target.prop)),
                fps,
                ear,
            ),
        })
        .collect();
    let tracks: Vec<PropertyTrack> = item
        .kind
        .registry
        .props()
        .map(|prop| property_track(prop, item.rest(prop), &written, fps))
        .collect();
    let integrals = item
        .kind
        .registry
        .rows
        .iter()
        .zip(&tracks)
        .filter(|(row, _)| row.integral)
        .map(|(row, track)| Arc::new(track.integral(fps, row.range)))
        .collect();
    ItemNode {
        kind: item.kind,
        id: item.id.clone(),
        tracks,
        integrals,
        settings: item.settings.clone(),
        items: item_nodes(&item.items, fps, ear),
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

/// When a layer shows and how its own clock runs against its list's.
fn timing(layer: &Layer, fps: f64) -> Timing {
    Timing {
        span: layer
            .span
            .map(|span| (span.start.to_frames(fps), span.end.to_frames(fps))),
        offset: layer.offset.to_frames(fps),
        stretch: layer.stretch,
        remaps: layer.value(Prop::REMAP).is_some()
            || layer
                .anim
                .iter()
                .any(|track| track.target.prop == Prop::REMAP),
    }
}

/// Compiles one layer into a node, on a clock at `fps` that ends at frame
/// `length`, with its content box, its parent's index, and the extent `at`
/// resolves in, and where its listening tracks' levels are.
fn node(
    layer: &Layer,
    scene: &Scene,
    (fps, length, canvas): (f64, f64, (f64, f64)),
    content: NodeContent,
    (bounds, parent, extent): (Rect, Option<usize>, Rect),
    ear: Ear<'_>,
) -> Node {
    // An entrance on the parts of a text is one of its animators instead.
    let entrance = entrance(layer, scene).filter(|(behaviour, _)| behaviour.parts.is_none());
    // An exit ends on the last frame of the layer's span, or the list's,
    // in its own time.
    let ends = Behaviour::last_shown(ear.timing.span.map_or(length, |(_, end)| end));
    let exit = layer.exit.as_ref().and_then(|behaviour| {
        let spec = scene.preset(&behaviour.preset)?;
        Some(Exit {
            behaviour,
            spec,
            end: ear.timing.local(ends),
        })
    });
    let moves = Moves {
        entrance,
        exit,
        canvas,
    };
    Node {
        id: layer.id.clone(),
        content,
        fill: layer.fill,
        bounds,
        parent,
        timing: ear.timing,
        tracks: tracks(layer, &moves, extent, bounds, fps, ear),
        follows_at: (
            follows_at(layer, &moves, Axis::X),
            follows_at(layer, &moves, Axis::Y),
        ),
        mask: item_nodes(&layer.mask, fps, ear),
        fx: item_nodes(&layer.fx, fps, ear),
        hidden: layer.hidden,
        adjust: layer.adjust,
        collapse: layer.collapse,
        matte: None,
        blend: layer.blend,
        mblur: layer.mblur,
        three_d: layer.three_d,
        inputs: Vec::new(),
        outlines: Vec::new(),
        pivots: Vec::new(),
    }
}

/// The layer's entrance and its preset, which parsing has checked exists.
fn entrance<'s>(layer: &'s Layer, scene: &'s Scene) -> Option<(&'s Behaviour, &'s PresetSpec)> {
    let behaviour = layer.enter.as_ref()?;
    scene
        .preset(&behaviour.preset)
        .map(|spec| (behaviour, spec))
}

/// A layer's exit, its preset, and the frame of its own time it ends on.
struct Exit<'s> {
    behaviour: &'s Behaviour,
    spec: &'s PresetSpec,
    end: f64,
}

/// What moves the whole layer besides `anim`: its entrance and its exit.
struct Moves<'s> {
    entrance: Option<(&'s Behaviour, &'s PresetSpec)>,
    exit: Option<Exit<'s>>,
    /// The width and height of the canvas the layer sits on, which starts
    /// such as `"+h"` move it by.
    canvas: (f64, f64),
}

impl Moves<'_> {
    /// Each target an entrance or exit moves and where it starts or ends.
    fn starts(&self) -> impl Iterator<Item = (Target, Start)> + '_ {
        let entrance = self
            .entrance
            .iter()
            .flat_map(|(_, spec)| spec.props.iter().copied());
        let exit = self
            .exit
            .iter()
            .flat_map(|e| e.spec.props.iter().map(|&(t, s)| (t, s.mirrored())));
        entrance.chain(exit)
    }
}

/// The keys one writer gives one target.
struct Written {
    target: Target,
    keys: Vec<KeySpec>,
}

/// The tracks of `layer`, one per property in registry order: rest, then
/// entrance and exit, then `anim`.
fn tracks(
    layer: &Layer,
    moves: &Moves<'_>,
    extent: Rect,
    own: Rect,
    fps: f64,
    ear: Ear<'_>,
) -> Vec<PropertyTrack> {
    let mut written: Vec<Written> = Vec::new();
    if let Some((behaviour, spec)) = moves.entrance {
        for &(target, from) in &spec.props {
            let rest = target.rest_of(layer.rest(target.prop, extent, own));
            written.push(Written {
                target,
                keys: entrance_keys(behaviour, from, rest, (fps, moves.canvas)),
            });
        }
    }
    // Parsing lets an exit share a target only once the entrance is over,
    // so its keys follow the entrance's.
    if let Some(exit) = &moves.exit {
        for &(target, to) in &exit.spec.props {
            let rest = target.rest_of(layer.rest(target.prop, extent, own));
            let keys = exit_keys(exit, to, rest, (fps, moves.canvas));
            match written.iter_mut().find(|w| w.target == target) {
                Some(w) => w.keys.extend(keys),
                None => written.push(Written { target, keys }),
            }
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
            keys: anim_keys(track, &rest, fps, ear),
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

/// One entrance target: from its start to `rest` over the behaviour's frames,
/// on a canvas of `canvas`.
fn entrance_keys(
    behaviour: &Behaviour,
    from: Start,
    rest: Value,
    (fps, canvas): (f64, (f64, f64)),
) -> Vec<KeySpec> {
    let (start, end) = behaviour.frames(fps);
    vec![
        KeySpec {
            frame: start,
            value: from.resolve(&rest, behaviour.distance, canvas),
            easing: Easing::All(behaviour.curve),
            path: None,
        },
        KeySpec::linear(end, rest),
    ]
}

/// One exit target: from `rest` to where the preset's start `to` mirrors,
/// over the frames that end with the layer's time, on a canvas of `canvas`.
fn exit_keys(
    exit: &Exit<'_>,
    to: Start,
    rest: Value,
    (fps, canvas): (f64, (f64, f64)),
) -> Vec<KeySpec> {
    let (start, end) = exit.behaviour.exit_frames(exit.end, fps);
    let gone = to
        .mirrored()
        .resolve(&rest, exit.behaviour.distance, canvas);
    vec![
        KeySpec {
            frame: start,
            value: rest,
            easing: Easing::All(exit.behaviour.curve),
            path: None,
        },
        KeySpec::linear(end, gone),
    ]
}

/// An `anim` track's keys with their times in frames, and values written
/// against the rest resolved against `rest`, the target's resting value. A
/// track that listens has a key every frame of its list's clock when `ear`
/// holds its levels, else its value at silence.
fn anim_keys(track: &AnimTrack, rest: &Value, fps: f64, ear: Ear<'_>) -> Vec<KeySpec> {
    if let (Some(listen), Some(first)) = (&track.listen, track.keys.first()) {
        let from = first.value.resolve(rest);
        let to = listen.to.resolve(rest);
        return ear
            .keys(listen, &from, &to)
            .unwrap_or_else(|| vec![KeySpec::linear(0.0, from)]);
    }
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
/// it, nor when the entrance starts it or the exit ends it at an absolute value.
fn follows_at(layer: &Layer, moves: &Moves<'_>, axis: Axis) -> bool {
    let on_axis = |target: Target| {
        target.prop.spec().rest == Rest::At && target.component.is_none_or(|i| i == axis.index())
    };
    let by_anim = layer.anim.iter().any(|track| on_axis(track.target));
    let by_start = moves
        .starts()
        .any(|(target, start)| on_axis(target) && matches!(start, Start::Value(_)));
    !by_anim && !by_start
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
