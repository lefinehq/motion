//! From an evaluated frame to the display list `nakkash-raster` draws
//! ([D34]): the one place that knows both scenes and pixels, so the scene
//! crates never touch pixels and the rasteriser never sees a scene.
//!
//! Every visible node becomes one layer under its canvas transform, with its
//! opacity and effects; what it draws becomes the layer's leaves, and an
//! image layer's leaf is its picture, which the caller decodes. A nested
//! composition is a layer clipped to the composition's canvas and isolated,
//! holding the composition's background and its own nodes at the time the
//! scene evaluated it. A collapsed one is neither clipped nor given its
//! background: its nodes draw straight into the canvas it sits in, under its
//! transform, and its effects run over that canvas in its coordinates, with
//! its own canvas where it lands there as its box (the box around it, when
//! it is turned).
//!
//! Frames blend gamma-encoded values, as the rasteriser does: the scene
//! format has no colour-space key yet, and linear-light blending ([D21]) is
//! refused by the rasteriser until it can draw it.
//!
//! [D21]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d21-linear-light-float-compositing
//! [D34]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

mod effect;
mod mask;
mod shape;
mod text;

use nakkash_format::key::{AUDIO, BLEND, IMAGE, VIDEO};
use nakkash_format::property::Prop;
use nakkash_format::shorthand::{MatteMode, RectShape};
use nakkash_format::{Cutout, ImageContent, Place, Result, VideoContent};
use nakkash_format::{Input, Reads, Value};
use nakkash_raster::kurbo::{Affine, Rect, RoundedRect, Shape};
use nakkash_raster::{
    BlendMode, Blending, Clip, Color, DrawStyle, Effect, FillRule, Frame, Heard, Homography,
    ImageDraw, Layer, Matte, MatteMode as RasterMatte, Node, Paint, PathDraw, Picture, Projection,
    Shared,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::pictures::Still;

use nakkash_format::shorthand::Shutter;
use nakkash_scene::space::{self, View};
use nakkash_scene::{
    CompGraph, ContentState, FrameState, ItemState, Node as GraphNode, NodeState, SceneGraph,
};

/// How closely curves are flattened into the outlines this module builds
/// from shapes, such as a rounded rectangle's corners, in pixels.
const TOLERANCE: f64 = 0.01;

/// Finds the picture or vector footage an image layer's `image` names.
pub(crate) type Pictures<'a> = dyn Fn(&str) -> Result<Still> + 'a;

/// Finds the sound an audio layer's `audio` plays, laid out for the mix.
pub(crate) type Sounds<'a> = dyn Fn(&str) -> Result<std::sync::Arc<nakkash_audio::Sound>> + 'a;

/// Finds the frame a video layer's `video` shows at a time in seconds, or
/// `None` outside the clip.
pub(crate) type Clips<'a> = dyn Fn(&str, Option<&Cutout>, f64) -> Result<Option<Picture>> + 'a;

/// What drawing one frame reads besides the frame's state: the compiled
/// scene, and the pictures and clips its image and video layers name.
#[derive(Clone, Copy)]
pub(crate) struct Sources<'a> {
    /// The compiled scene.
    pub(crate) graph: &'a SceneGraph,
    /// The pictures of image layers.
    pub(crate) pictures: &'a Pictures<'a>,
    /// The frames of video layers.
    pub(crate) clips: &'a Clips<'a>,
    /// The sounds of audio layers, stereo at the mix rate.
    pub(crate) sounds: &'a Sounds<'a>,
    /// The moments motion blur has evaluated so far.
    pub(crate) moments: &'a Moments,
    /// The compositions drawn so far this frame, by the time they show.
    pub(crate) instances: &'a Instances,
}

/// The children of each composition a frame shows, by its index and the
/// frame of its own clock it shows: a composition at one frame draws the
/// same wherever it is shown, so its children are made once a frame, and
/// the layers that show it share them and one [`Shared`] mark, which lets
/// the rasteriser draw them once at each transform.
#[derive(Default)]
pub(crate) struct Instances(RefCell<HashMap<(usize, u64), Instance>>);

/// The mark and the children of one composition at one frame.
type Instance = (Shared, Vec<Node>);

impl Instances {
    /// The mark and the children of composition `index` at `frame`, as
    /// `make` makes them the first time.
    fn of(
        &self,
        index: usize,
        frame: f64,
        make: impl FnOnce() -> Result<Vec<Node>>,
    ) -> Result<(Shared, Vec<Node>)> {
        let key = (index, frame.to_bits());
        if let Some((shared, children)) = self.0.borrow().get(&key) {
            return Ok((shared.clone(), children.clone()));
        }
        let children = make()?;
        let shared = Shared::new();
        self.0
            .borrow_mut()
            .insert(key, (shared.clone(), children.clone()));
        Ok((shared, children))
    }
}

/// A canvas the engine draws: the scene's root, or a composition of
/// `graph.comps`, which motion blur evaluates at other moments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Canvas {
    /// The scene's root.
    Root,
    /// The composition at this index.
    Comp(usize),
}

impl Canvas {
    /// Its compiled composition.
    fn graph(self, graph: &SceneGraph) -> &CompGraph {
        match self {
            Canvas::Root => &graph.root,
            Canvas::Comp(index) => &graph.comps[index],
        }
    }
}

/// States of canvases at the moments motion blur samples, kept for one
/// frame so layers that share a moment evaluate it once.
#[derive(Default)]
pub(crate) struct Moments(RefCell<HashMap<(Canvas, u64), Rc<FrameState>>>);

impl Moments {
    /// `canvas` evaluated at its own `frame`.
    fn at(&self, graph: &SceneGraph, canvas: Canvas, frame: f64) -> Rc<FrameState> {
        let key = (canvas, frame.to_bits());
        if let Some(state) = self.0.borrow().get(&key) {
            return Rc::clone(state);
        }
        let state = Rc::new(match canvas {
            Canvas::Root => nakkash_scene::evaluate(graph, frame),
            Canvas::Comp(index) => nakkash_scene::evaluate_comp(graph, index, frame),
        });
        self.0.borrow_mut().insert(key, Rc::clone(&state));
        state
    }
}

/// The display list of `state`, one frame of `sources.graph`'s scene, drawn
/// over the root composition's background (a clear one draws nothing), with
/// the image layers' pictures and the video layers' frames from `sources`.
///
/// # Errors
/// A diagnostic for an effect the rasteriser has no row for, or a parameter
/// it rejects, at the effect's place in the layer's `fx`; or the one
/// `sources` returns, at the layer's `image` or `video`.
pub(crate) fn display_list(sources: Sources<'_>, state: &FrameState) -> Result<Frame> {
    let graph = sources.graph;
    let background = graph.root.background;
    Ok(Frame {
        background: opaque_enough(background).then(|| color(background)),
        blending: Blending::Gamma,
        nodes: nodes(sources, Canvas::Root, state)?,
    })
}

/// The layers of one canvas's nodes, back to front: `at` is the canvas,
/// `state` its nodes' state at this frame.
fn nodes(sources: Sources<'_>, at: Canvas, state: &FrameState) -> Result<Vec<Node>> {
    let read = ReadLayers::default();
    paint_order(state)
        .into_iter()
        .filter_map(|i| matted(sources, at, state, i, &read).transpose())
        .collect()
}

/// The layers of one canvas that its effects read, each drawn once and
/// shared by every effect that reads it, so the rasteriser draws it once
/// too: by the layer's index in the canvas.
#[derive(Default)]
struct ReadLayers(RefCell<HashMap<usize, Arc<Node>>>);

impl ReadLayers {
    /// The node of layer `source`, as `draw` makes it the first time.
    fn node(&self, source: usize, draw: impl FnOnce() -> Result<Node>) -> Result<Arc<Node>> {
        if let Some(node) = self.0.borrow().get(&source) {
            return Ok(Arc::clone(node));
        }
        let node = Arc::new(draw()?);
        self.0.borrow_mut().insert(source, Arc::clone(&node));
        Ok(node)
    }
}

/// The visible nodes of `state` that draw, in the order they draw ([D58]): list
/// order, except that each run of adjacent 3D nodes draws back to front
/// ([`back_to_front`]); a flat node between two runs keeps them apart.
///
/// [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
pub(crate) fn paint_order(state: &FrameState) -> Vec<usize> {
    // A node that draws nothing, such as a pivot, splits no run.
    let visible: Vec<usize> = (0..state.nodes.len())
        .filter(|&i| state.nodes[i].visible())
        .filter(|&i| {
            !matches!(
                state.nodes[i].content,
                ContentState::Pivot
                    | ContentState::Audio
                    | ContentState::Camera
                    | ContentState::Comp(_, None)
            )
        })
        .collect();
    let deep = |i: usize| state.nodes[i].space.is_some();
    let mut order = Vec::with_capacity(visible.len());
    for run in visible.chunk_by(|&a, &b| deep(a) == deep(b)) {
        match deep(run[0]) {
            true => order.extend(back_to_front(state, run)),
            false => order.extend_from_slice(run),
        }
    }
    order
}

/// A 3D node as the paint order sees it: its box's corners in the camera's
/// space, where the box shows on the canvas, and how deep its centre is.
struct Seen {
    corners: [glam::DVec3; 4],
    footprint: Option<Rect>,
    depth: f64,
}

/// The 3D nodes `run` of `state`, in list order, in the order they draw.
/// Where the boxes of two show over each other, the one wholly behind the
/// other's plane draws first ([`space::depth_order`]), and of two in one
/// plane the first in the list; this holds a layer laid just in front of a
/// larger one in front, however far apart their centres are. Otherwise, and
/// to break a cycle, the node whose centre is deepest draws first, then the
/// first in the list. Planes that cross are drawn whole, not cut.
fn back_to_front(state: &FrameState, run: &[usize]) -> Vec<usize> {
    let seen: Vec<Seen> = run
        .iter()
        .map(|&i| {
            let node = &state.nodes[i];
            let world = node.space.expect("a 3D node");
            let bounds = node.local_bounds;
            Seen {
                corners: state.view.corners(world, bounds),
                footprint: space::bbox(&space::outline(state.view.plane(world), bounds)),
                depth: state.view.depth(world, bounds.center()),
            }
        })
        .collect();
    // `after[a]` holds the nodes that draw after `a`; `waiting[b]` counts
    // the nodes `b` draws after that have not drawn yet.
    let mut after = vec![Vec::new(); run.len()];
    let mut waiting = vec![0_usize; run.len()];
    for a in 0..run.len() {
        for b in a + 1..run.len() {
            let overlap = match (seen[a].footprint, seen[b].footprint) {
                (Some(fa), Some(fb)) => fa.intersect(fb).area() > 0.0,
                _ => false,
            };
            if !overlap {
                continue;
            }
            let (first, then) = match space::depth_order(&seen[a].corners, &seen[b].corners) {
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal) => (a, b),
                Some(std::cmp::Ordering::Greater) => (b, a),
                None => continue,
            };
            after[first].push(then);
            waiting[then] += 1;
        }
    }
    // The deepest first, then the first in the list.
    let deeper = |a: usize, b: usize| {
        seen[a]
            .depth
            .total_cmp(&seen[b].depth)
            .then_with(|| b.cmp(&a))
    };
    let mut left: Vec<usize> = (0..run.len()).collect();
    let mut order = Vec::with_capacity(run.len());
    while !left.is_empty() {
        let free = left.iter().copied().filter(|&n| waiting[n] == 0);
        let next = free
            .max_by(|&a, &b| deeper(a, b))
            .or_else(|| left.iter().copied().max_by(|&a, &b| deeper(a, b)))
            .expect("a node left");
        left.retain(|&n| n != next);
        for &then in &after[next] {
            waiting[then] = waiting[then].saturating_sub(1);
        }
        order.push(run[next]);
    }
    order
}

/// The canvas of `comp`, from its top left.
fn canvas(comp: &CompGraph) -> Rect {
    Rect::new(0.0, 0.0, comp.width, comp.height)
}

/// The layer of node `i` of the canvas `at`, with its matte if it has
/// one, or `None` when it draws nothing.
fn matted(
    sources: Sources<'_>,
    at: Canvas,
    state: &FrameState,
    i: usize,
    read: &ReadLayers,
) -> Result<Option<Node>> {
    let list = &at.graph(sources.graph).nodes;
    let drawn = match blurs(&list[i]) {
        true => blurred(sources, at, state, i)?,
        false => layer(sources, &list[i], &state.view, &state.nodes[i])?,
    };
    let Some(Node::Layer(mut drawn)) = drawn else {
        return Ok(None);
    };
    hear(sources, list, state, i, &mut drawn.effects)?;
    // An adjusting layer applies its effects to what is below it, where it
    // draws.
    drawn.adjust |= list[i].adjust;
    drawn.blend = BlendMode::named(list[i].blend)
        .map_err(|e| e.at(BLEND).under(Place::of_layer(&state.nodes[i].id)))?;
    // An effect that reads another layer reads its drawing in its own
    // coordinates, with its effects but not its transform, opacity or
    // matte; nothing outside its span. A collapsed composition is read as
    // its composition, uncollapsed: collapsing changes where it draws, not
    // what it is.
    // One that reads its own layer when it names none reads the layer's
    // drawing before its effects, cut by its masks, in its own
    // coordinates.
    let items = &state.nodes[i].fx;
    let own = items
        .iter()
        .any(|item| {
            item.kind
                .input
                .is_some_and(|input| input.reads == Reads::Laid)
        })
        .then(|| Layer {
            transform: Affine::IDENTITY,
            projection: None,
            opacity: 1.0,
            blend: BlendMode::default(),
            effects: Vec::new(),
            matte: None,
            adjust: false,
            motion: Vec::new(),
            bounds: None,
            ..drawn.clone()
        });
    for ((effect, input), item) in drawn.effects.iter_mut().zip(&list[i].inputs).zip(items) {
        let Some(source) = *input else {
            if let (
                Some(own),
                Some(Input {
                    reads: Reads::Laid, ..
                }),
            ) = (&own, item.kind.input)
            {
                *effect = effect.clone().with_input(Node::Layer(own.clone()));
            }
            continue;
        };
        let node = read.node(source, || {
            let drawn = match state.nodes[source].shown {
                true => drawing(
                    sources,
                    &list[source],
                    &state.view,
                    &state.nodes[source],
                    false,
                )?,
                false => None,
            };
            Ok(Node::Layer(match drawn {
                Some(Node::Layer(drawn)) => Layer {
                    transform: Affine::IDENTITY,
                    projection: None,
                    opacity: 1.0,
                    ..drawn
                },
                _ => Layer::new(Vec::new()),
            }))
        })?;
        // One that reads a laid layer reads it already laid on its own
        // effect box, so nothing it reads falls outside the frame. That
        // read is laid for this effect alone, so it is not shared.
        let laid_read = matches!(
            item.kind.input,
            Some(Input {
                reads: Reads::Laid,
                ..
            })
        );
        let node = match (laid_read, &*node) {
            (true, Node::Layer(read)) => {
                let around = canvas(at.graph(sources.graph));
                let from = effect_box(sources, &state.nodes[source], around);
                // A collapsed composition runs its effects in the
                // coordinates of the canvas it draws into.
                let onto = match list[i].collapse {
                    true if matches!(state.nodes[i].content, ContentState::Comp(..)) => around,
                    _ => effect_box(sources, &state.nodes[i], around),
                };
                Arc::new(Node::Layer(Layer {
                    transform: laid(item, from, onto),
                    bounds: None,
                    ..read.clone()
                }))
            }
            _ => node,
        };
        *effect = effect.clone().with_input(node);
    }
    let Some((source, mode)) = list[i].matte else {
        return Ok(Some(Node::Layer(drawn)));
    };
    // The matte layer draws into the mask whether or not it is hidden, and
    // nothing outside its span; it may be matted in turn.
    let matte = &state.nodes[source];
    let source = match matte.shown {
        true => matted(sources, at, state, source, read)?,
        false => None,
    };
    Ok(Some(Node::Layer(Layer {
        matte: Some(Matte {
            source: Box::new(source.unwrap_or_else(|| Node::Layer(Layer::new(Vec::new())))),
            mode: match mode {
                MatteMode::Alpha => RasterMatte::Alpha,
                MatteMode::AlphaInverted => RasterMatte::AlphaInverted,
                MatteMode::Luma => RasterMatte::Luma,
                MatteMode::LumaInverted => RasterMatte::LumaInverted,
            },
        }),
        ..drawn
    })))
}

/// A node's effect box in its own coordinates: what its content fills (a
/// rect, an image's or video's frame, a composition's canvas), or for
/// content without a box of its own, `around`, the canvas it sits in, with
/// its middle on the node's origin.
fn effect_box(sources: Sources<'_>, node: &NodeState, around: Rect) -> Rect {
    match &node.content {
        ContentState::Rect(shape) => Rect::new(0.0, 0.0, shape.width, shape.height),
        ContentState::Image(content) => content.frame,
        ContentState::Video { video, .. } => video.frame,
        ContentState::Comp(index, _) => canvas(&sources.graph.comps[*index]),
        _ => around - around.center().to_vec2(),
    }
}

/// From the coordinates of a layer an effect reads, whose effect box is
/// `from`, to those of the layer it is on, whose box is `onto`: the box
/// stretched over the other, or its middle on the other's, as the
/// effect's `behaviour` setting says.
fn laid(item: &ItemState, from: Rect, onto: Rect) -> Affine {
    let behaviour = item
        .kind
        .settings
        .iter()
        .zip(&item.settings)
        .find(|(setting, _)| setting.name == "behaviour")
        .and_then(|(setting, &choice)| setting.choices.get(choice).copied());
    match behaviour {
        Some("stretch") if from.width() > 0.0 && from.height() > 0.0 => {
            Affine::translate(onto.origin().to_vec2())
                * Affine::scale_non_uniform(
                    onto.width() / from.width(),
                    onto.height() / from.height(),
                )
                * Affine::translate(-from.origin().to_vec2())
        }
        _ => Affine::translate(onto.center() - from.center()),
    }
}

/// Gives each effect of node `i` that reads a layer's sound the window it
/// asks for: `duration` ms of what that layer plays, centred `offset` ms
/// after the canvas's time, mixed in the canvas's clock.
///
/// # Errors
/// A sound file that cannot be read, at its audio layer.
fn hear(
    sources: Sources<'_>,
    list: &[GraphNode],
    state: &FrameState,
    i: usize,
    effects: &mut [Effect],
) -> Result<()> {
    let items = &state.nodes[i].fx;
    for ((effect, input), item) in effects.iter_mut().zip(&list[i].inputs).zip(items) {
        let (
            Some(source),
            Some(Input {
                reads: Reads::Sound,
                ..
            }),
        ) = (*input, item.kind.input)
        else {
            continue;
        };
        let value = |name: &str| {
            item.kind
                .registry
                .prop(name)
                .and_then(|prop| item.values.get(prop.index()))
                .and_then(Value::as_number)
                .unwrap_or(0.0)
        };
        let (duration, offset) = (value("duration") / 1000.0, value("offset") / 1000.0);
        let centre = state.frame / state.fps + offset;
        let frames = (centre - duration / 2.0) * state.fps..(centre + duration / 2.0) * state.fps;
        let routes = nakkash_scene::routes_from(sources.graph, list, source);
        let canvas = crate::mix::Canvas {
            nodes: list,
            fps: state.fps,
        };
        let sound = |route: &nakkash_scene::Route| {
            (sources.sounds)(route.path())
                .map_err(|e| e.at(AUDIO).under(Place::of_layer(route.id(sources.graph))))
        };
        let heard = crate::mix::mix_in(sources.graph, canvas, &routes, frames, sound)?;
        let [left, right] = heard.channels() else {
            continue;
        };
        *effect = effect.clone().with_sound(Heard {
            left: left.clone(),
            right: right.clone(),
            rate: heard.rate() as f32,
        });
    }
    Ok(())
}

/// Whether `node` is drawn blurred along its motion: it asks to be, and it
/// is neither an adjusting layer nor one whose effects read another layer,
/// which are drawn at the frame's moment only.
fn blurs(node: &GraphNode) -> bool {
    node.mblur && !node.adjust && node.inputs.iter().all(Option::is_none)
}

/// How many times its shutter's samples a `comp` layer, or one with
/// effects, may take, drawn once and moved or drawn at each moment: each
/// is a pass over the frame's pixels, so fewer than a drawing without
/// effects made at every moment.
const MOVED_GROWTH: u32 = 2;

/// Node `i` of the canvas `at` blurred along its motion ([D57]), over the
/// canvas's open shutter. Its corners' travel over the shutter, in px,
/// decides the number of moments `n`: the shutter's samples, up to
/// [`Shutter::MAX_GROWTH`] times as many. Whether it draws is decided at
/// the frame: a moment outside its span still moves it, with its drawing
/// at the frame, so a layer does not fade in or out at its ends.
///
/// - A shape, text, rect or video layer without effects, or an image
///   layer with masks, is drawn at each moment, at `1/n` of its opacity
///   and added, in a group of its own, so what moves inside it blurs too.
/// - An image layer without masks or effects is drawn once at the frame
///   and moved to where each moment puts it (`Layer::motion`, or
///   `Projection::samples` for a 3D one): its picture is the same at every
///   moment, so this is the same drawing, held once.
/// - A `comp` layer, or one with effects, is drawn once at the frame and
///   moved the same way, up to [`MOVED_GROWTH`] times its samples. Its
///   content is its composition's, which blurs by its own shutter, as a
///   nested composition's does in the projects the import reads.
/// - A layer with an effect whose drawing runs on with time
///   (`ItemKind::timed`, such as particles) is drawn at each moment as a
///   shape is, effects and all, up to [`MOVED_GROWTH`] times its samples:
///   moved once, its particles would stand still over the shutter.
///
/// [D57]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter
fn blurred(sources: Sources<'_>, at: Canvas, state: &FrameState, i: usize) -> Result<Option<Node>> {
    let graph = sources.graph;
    let comp = at.graph(graph);
    let node = &comp.nodes[i];
    let shutter = comp.shutter;
    if !shutter.open() {
        return layer(sources, node, &state.view, &state.nodes[i]);
    }
    let moment = |frame: f64| sources.moments.at(graph, at, frame);
    let opens = moment(state.frame + shutter.phase / 360.0);
    let closes = moment(state.frame + (shutter.phase + shutter.angle) / 360.0);
    let frame_node = &state.nodes[i];
    // A picture without masks draws the same at every moment, so drawing it
    // once and moving it is exact, and it holds one drawing rather than one
    // per moment: a 3D picture seen up close took a gigabyte a frame.
    let still = matches!(frame_node.content, ContentState::Image(_)) && frame_node.mask.is_empty();
    let comp = matches!(frame_node.content, ContentState::Comp(..));
    // An effect whose drawing runs on with time, such as particles, draws
    // something else at every moment, which a drawing moved as a whole
    // cannot show: its layer is drawn at each moment, at its time then.
    let timed = frame_node.fx.iter().any(|item| item.kind.timed);
    let moved = !timed && (still || comp || !frame_node.fx.is_empty());
    // A moment drawn with effects, once and moved or afresh, is a pass over
    // the frame's pixels.
    let growth = match !still && (comp || !frame_node.fx.is_empty()) {
        true => MOVED_GROWTH,
        false => Shutter::MAX_GROWTH,
    };
    let most = shutter.samples * growth;
    let travel = travel(&opens.nodes[i], &closes.nodes[i]).ceil();
    let n = match travel.is_finite() {
        true => (travel.min(f64::from(most)) as u32).max(shutter.samples),
        false => most,
    };
    if moved {
        let Some(Node::Layer(drawn)) = layer(sources, node, &state.view, frame_node)? else {
            return Ok(None);
        };
        // A 3D node is laid through its plane at each moment, as a whole
        // map: its plane at the frame may be edge-on and have no inverse.
        if let Some(projection) = drawn.projection.clone() {
            let samples = shutter
                .moments(state.frame, n)
                .filter_map(|frame| moment(frame).nodes[i].plane.map(homography))
                .collect();
            return Ok(Some(Node::Layer(Layer {
                projection: Some(Projection {
                    samples,
                    ..projection
                }),
                ..drawn
            })));
        }
        let back = frame_node.transform.inverse();
        let motion = shutter
            .moments(state.frame, n)
            .map(|frame| moment(frame).nodes[i].transform * back)
            .collect();
        return Ok(Some(Node::Layer(Layer { motion, ..drawn })));
    }
    let mut samples = Vec::with_capacity(n as usize);
    for frame in shutter.moments(state.frame, n) {
        let at_moment = moment(frame);
        let sampled = &at_moment.nodes[i];
        let outside;
        let sampled = match sampled.shown {
            true => sampled,
            false => {
                outside = NodeState {
                    transform: sampled.transform,
                    ..frame_node.clone()
                };
                &outside
            }
        };
        if let Some(Node::Layer(drawn)) = layer(sources, node, &at_moment.view, sampled)? {
            samples.push(Node::Layer(Layer {
                opacity: drawn.opacity / n as f32,
                blend: BlendMode::Add,
                ..drawn
            }));
        }
    }
    Ok((!samples.is_empty()).then(|| {
        Node::Layer(Layer {
            isolate: true,
            ..Layer::new(samples)
        })
    }))
}

/// How far, in px of the canvas, the corners of a node's box move from
/// `from` to `to`: the most any of them moves.
fn travel(from: &NodeState, to: &NodeState) -> f64 {
    // A 3D node's corners as the camera sees them; one behind the camera
    // counts as far, so the layer takes the most samples.
    let corners = |node: &NodeState| {
        let b = node.local_bounds;
        [(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)].map(|(x, y)| {
            let p = nakkash_raster::kurbo::Point::new(x, y);
            match node.plane {
                None => node.transform * p,
                Some(plane) => space::project(plane, p).unwrap_or(
                    nakkash_raster::kurbo::Point::new(f64::INFINITY, f64::INFINITY),
                ),
            }
        })
    };
    corners(from)
        .into_iter()
        .zip(corners(to))
        .map(|(a, b)| a.distance(b))
        .fold(0.0, f64::max)
}

/// The layer of one node, compiled as `compiled`, without its matte, or
/// `None` when it draws nothing.
fn layer(
    sources: Sources<'_>,
    compiled: &GraphNode,
    view: &View,
    node: &NodeState,
) -> Result<Option<Node>> {
    drawing(sources, compiled, view, node, compiled.collapse)
}

/// The layer of one node as [`layer`] draws it, its composition collapsed
/// into the canvas it sits in only when `collapse` says so.
fn drawing(
    sources: Sources<'_>,
    compiled: &GraphNode,
    view: &View,
    node: &NodeState,
    collapse: bool,
) -> Result<Option<Node>> {
    let opacity = node.number(Prop::OPACITY) as f32;
    if opacity <= 0.0 {
        return Ok(None);
    }
    let mut clip = None;
    let mut isolate = false;
    let mut bounds = None;
    let mut shared = None;
    let children = match &node.content {
        ContentState::Rect(shape) => {
            bounds = Some(Rect::new(0.0, 0.0, shape.width, shape.height));
            vec![rect(shape, node.fill)]
        }
        ContentState::Shapes(draws) => shape::draws(draws),
        ContentState::Text { text, glyphs } => text::draws(text, glyphs),
        ContentState::Image(content) => {
            bounds = Some(content.frame);
            image(content, sources.pictures)
                .map_err(|e| e.under(Place::of_layer(&node.id)))?
                .into_iter()
                .collect()
        }
        ContentState::Video {
            video: content,
            seconds,
        } => {
            bounds = Some(content.frame);
            video(content, *seconds, sources.clips)
                .map_err(|e| e.under(Place::of_layer(&node.id)))?
                .into_iter()
                .collect()
        }
        ContentState::Comp(index, Some(inner)) if collapse => {
            let comp = &sources.graph.comps[*index];
            let inner = joined(inner, view, node);
            let children = nodes(sources, Canvas::Comp(*index), &inner)?;
            if children.is_empty() {
                return Ok(None);
            }
            let effects = effect::effects(node, &compiled.outlines)
                .map_err(|e| e.under(Place::of_layer(&node.id)))?;
            // Its own blend modes reach what is below unless its effects
            // need its drawing on its own, or an adjusting layer inside
            // needs only its composition below it.
            let isolate = !effects.is_empty() || comp.nodes.iter().any(|n| n.adjust);
            return Ok(Some(Node::Layer(Layer {
                opacity: opacity.min(1.0),
                effects,
                isolate,
                // Its box is its composition's canvas where it lands: the
                // box effects tile and wrap, as an uncollapsed one's.
                bounds: Some(node.transform.transform_rect_bbox(canvas(comp))),
                time: node.seconds,
                ..Layer::new(moved(node.transform, children))
            })));
        }
        ContentState::Comp(index, Some(inner)) => {
            let comp = &sources.graph.comps[*index];
            let canvas = canvas(comp);
            clip = Some(Clip::Rect(canvas));
            bounds = Some(canvas);
            isolate = true;
            // Every layer of the frame that shows this composition at this
            // time draws the same children: they are made once and the
            // layers share them.
            let (instance, children) = sources.instances.of(*index, inner.frame, || {
                let mut children = Vec::new();
                if opaque_enough(comp.background) {
                    children.push(fill(canvas.to_path(TOLERANCE), color(comp.background)));
                }
                children.extend(nodes(sources, Canvas::Comp(*index), inner)?);
                Ok(children)
            })?;
            shared = Some(instance);
            children
        }
        ContentState::Comp(_, None)
        | ContentState::Audio
        | ContentState::Pivot
        | ContentState::Camera => Vec::new(),
        ContentState::Filter => {
            let effects = effect::effects(node, &compiled.outlines)
                .map_err(|e| e.under(Place::of_layer(&node.id)))?;
            return Ok(Some(Node::Layer(Layer {
                transform: node.transform,
                projection: projection(node),
                opacity: opacity.min(1.0),
                masks: mask::masks(&node.mask),
                effects,
                adjust: true,
                time: node.seconds,
                ..Layer::new(Vec::new())
            })));
        }
    };
    if children.is_empty() {
        // Effects change what the layer drew, so without content there is
        // nothing for them to change.
        return Ok(None);
    }
    let effects = effect::effects(node, &compiled.outlines)
        .map_err(|e| e.under(Place::of_layer(&node.id)))?;
    Ok(Some(Node::Layer(Layer {
        transform: node.transform,
        projection: projection(node),
        opacity: opacity.min(1.0),
        clip,
        masks: mask::masks(&node.mask),
        effects,
        isolate,
        bounds,
        time: node.seconds,
        shared,
        ..Layer::new(children)
    })))
}

/// The state of a collapsed composition's nodes as they join the canvas
/// the `comp` node `node` sits in, seen through that canvas's `view`
/// ([D58]): each 3D node's matrix of space continues the node's own (its
/// matrix when it is 3D, else its transform at depth 0), and its plane is
/// the outer camera's, in the coordinates of the group `node`'s transform
/// moves.
///
/// [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
fn joined(inner: &FrameState, view: &View, node: &NodeState) -> FrameState {
    let outer = node.space.unwrap_or_else(|| space::lift(node.transform));
    let back = space::projective(node.transform).inverse();
    let mut joined = inner.clone();
    joined.view = *view;
    for inside in &mut joined.nodes {
        let Some(own) = inside.space else {
            continue;
        };
        let world = outer * own;
        inside.space = Some(world);
        inside.plane = inside.plane.map(|_| back * view.plane(world));
    }
    joined
}

/// How a 3D node's drawing is laid onto its canvas: its plane through the
/// camera ([D58]); `None` for a flat node, which its transform places.
///
/// [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
fn projection(node: &NodeState) -> Option<Projection> {
    node.plane.map(|plane| Projection::new(homography(plane)))
}

/// A scene's projective matrix as the rasteriser's.
fn homography(plane: glam::DMat3) -> Homography {
    Homography::new(plane.transpose().to_cols_array_2d())
}

/// A rect node: its box, with rounded corners, filled with its colour.
fn rect(shape: &RectShape, rgba: [f64; 4]) -> Node {
    let (width, height) = (shape.width, shape.height);
    // Corners larger than half the shorter side would overlap; they meet instead.
    let radius = shape.radius.clamp(0.0, width.min(height) / 2.0);
    let outline = RoundedRect::new(0.0, 0.0, width, height, radius).to_path(TOLERANCE);
    fill(outline, color(rgba))
}

/// An image node: its picture, or the part `crop` names, fitted to its box
/// as its `fit` says; `None` for a layer with no file yet. Vector footage
/// ([D70]) is drawn into a picture the size of the rectangle it lands on
/// once, which is then transformed like any picture, or, `continuous`,
/// laid over that rectangle as vectors, drawn at the output's resolution
/// every frame. The picture's errors are placed at `image`, and name the
/// file.
///
/// [D70]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines
fn image(content: &ImageContent, pictures: &Pictures<'_>) -> Result<Option<Node>> {
    let Some(path) = &content.path else {
        return Ok(None);
    };
    let frame = content.frame;
    match pictures(path).map_err(|e| e.at(IMAGE))? {
        Still::Picture(picture) => {
            let whole = whole(&picture);
            let fitted = content.fit.place(content.crop.unwrap_or(whole), frame);
            // A picture shown whole is laid over its rectangle; a part of
            // one is cut, as a crop always was.
            let part = (content.crop.is_some() || fitted.shown != whole).then_some(fitted.shown);
            Ok(Some(placed(picture, part, fitted.rect)))
        }
        Still::Drawing(drawing) => {
            let region = content.crop.unwrap_or_else(|| drawing.bounds());
            let fitted = content.fit.place(region, frame);
            if content.continuous {
                return Ok(Some(drawing.placed(fitted.shown, fitted.rect)));
            }
            let side = |length: f64| length.abs().ceil().max(1.0) as u32;
            let picture = drawing
                .picture(
                    fitted.shown,
                    side(fitted.rect.width()),
                    side(fitted.rect.height()),
                )
                .map_err(|e| e.at(IMAGE))?;
            Ok(Some(Node::Image(ImageDraw {
                picture,
                rect: fitted.rect,
            })))
        }
    }
}

/// A video node: the frame its clip shows `seconds` into the layer's own
/// time, fitted to its box as its `fit` says; `None` for a layer with no
/// file yet, and before or after its clip. With a `cutout`, the frame's
/// alpha is the baked matte's times its own. The clip's errors are placed
/// at `video`, and name the file; the matte's at `cutout`.
fn video(content: &VideoContent, seconds: f64, clips: &Clips<'_>) -> Result<Option<Node>> {
    let Some(path) = &content.path else {
        return Ok(None);
    };
    let placed_at_video = |e: nakkash_format::Diagnostic| match e.path {
        Some(_) => e,
        None => e.at(VIDEO),
    };
    let Some(picture) = clips(path, content.cutout.as_ref(), seconds).map_err(placed_at_video)?
    else {
        return Ok(None);
    };
    let whole = whole(&picture);
    let fitted = content.fit.place(whole, content.frame);
    let part = (fitted.shown != whole).then_some(fitted.shown);
    Ok(Some(placed(picture, part, fitted.rect)))
}

/// The whole of `picture`, in its pixels from its top-left corner.
fn whole(picture: &Picture) -> Rect {
    Rect::new(
        0.0,
        0.0,
        f64::from(picture.width()),
        f64::from(picture.height()),
    )
}

/// `picture` laid over `rect`: all of it, stretched, or, when `part` names
/// a part of it in its pixels, the whole picture placed so that part lands
/// on `rect`, and cut to `rect`.
fn placed(picture: Picture, part: Option<Rect>, rect: Rect) -> Node {
    let Some(part) = part else {
        return Node::Image(ImageDraw { picture, rect });
    };
    let (sx, sy) = (rect.width() / part.width(), rect.height() / part.height());
    let x0 = rect.x0 - part.x0 * sx;
    let y0 = rect.y0 - part.y0 * sy;
    let whole = Rect::new(
        x0,
        y0,
        x0 + f64::from(picture.width()) * sx,
        y0 + f64::from(picture.height()) * sy,
    );
    Node::Layer(Layer {
        clip: Some(Clip::Rect(rect)),
        ..Layer::new(vec![Node::Image(ImageDraw {
            picture,
            rect: whole,
        })])
    })
}

/// `path` filled with `color` by the non-zero rule.
fn fill(path: nakkash_raster::kurbo::BezPath, color: Color) -> Node {
    Node::Path(PathDraw {
        path,
        paint: Paint::Solid(color),
        style: DrawStyle::Fill(FillRule::NonZero),
    })
}

/// A scene colour, straight RGBA from 0 to 1, as the rasteriser's.
fn color([r, g, b, a]: [f64; 4]) -> Color {
    Color::rgba(r as f32, g as f32, b as f32, a as f32)
}

/// Whether a colour leaves any mark: its alpha is above zero.
fn opaque_enough(rgba: [f64; 4]) -> bool {
    rgba[3] > 0.0
}

/// Wraps `children` in a layer that only moves them, or returns them as they
/// are under the identity.
fn moved(transform: Affine, children: Vec<Node>) -> Vec<Node> {
    if transform == Affine::IDENTITY {
        return children;
    }
    vec![Node::Layer(Layer {
        transform,
        ..Layer::new(children)
    })]
}

#[cfg(test)]
#[path = "../../tests/unit/render/mod.rs"]
mod tests;
