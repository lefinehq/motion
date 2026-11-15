//! From an evaluated frame to the display list `motion-raster` draws
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
//! transform, and its effects run over that canvas in its coordinates.
//!
//! Frames blend gamma-encoded values, as the rasteriser does: the scene
//! format has no colour-space key yet, and linear-light blending ([D21]) is
//! refused by the rasteriser until it can draw it.
//!
//! [D21]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d21-linear-light-float-compositing
//! [D34]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d34-a-layer-tree-renderer-on-vello_cpu

mod effect;
mod mask;
mod shape;
mod text;

use motion_format::key::{AUDIO, BLEND, IMAGE, VIDEO};
use motion_format::property::Prop;
use motion_format::shorthand::{MatteMode, RectShape};
use motion_format::{ImageContent, Place, Result};
use motion_format::{Input, Reads, Value};
use motion_raster::kurbo::{Affine, Rect, RoundedRect, Shape};
use motion_raster::{
    BlendMode, Blending, Clip, Color, DrawStyle, Effect, FillRule, Frame, Heard, Homography,
    ImageDraw, Layer, Matte, MatteMode as RasterMatte, Node, Paint, PathDraw, Picture, Projection,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use motion_format::shorthand::Shutter;
use motion_scene::space::{self, View};
use motion_scene::{CompGraph, ContentState, FrameState, Node as GraphNode, NodeState, SceneGraph};

/// How closely curves are flattened into the outlines this module builds
/// from shapes, such as a rounded rectangle's corners, in pixels.
const TOLERANCE: f64 = 0.01;

/// Finds the picture an image layer's `image` names.
pub(crate) type Pictures<'a> = dyn Fn(&str) -> Result<Picture> + 'a;

/// Finds the sound an audio layer's `audio` plays, laid out for the mix.
pub(crate) type Sounds<'a> = dyn Fn(&str) -> Result<std::sync::Arc<motion_audio::Sound>> + 'a;

/// Finds the frame a video layer's `video` shows at a time in seconds, or
/// `None` outside the clip.
pub(crate) type Clips<'a> = dyn Fn(&str, f64) -> Result<Option<Picture>> + 'a;

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
            Canvas::Root => motion_scene::evaluate(graph, frame),
            Canvas::Comp(index) => motion_scene::evaluate_comp(graph, index, frame),
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
        nodes: nodes(sources, Canvas::Root, canvas(&graph.root), state)?,
    })
}

/// The layers of one canvas's nodes, back to front: `list` is the canvas's
/// compiled nodes, `area` the canvas, `state` their state at this frame.
fn nodes(sources: Sources<'_>, at: Canvas, area: Rect, state: &FrameState) -> Result<Vec<Node>> {
    paint_order(state)
        .into_iter()
        .filter_map(|i| matted(sources, at, area, state, i).transpose())
        .collect()
}

/// The visible nodes of `state` that draw, in the order they draw ([D58]): list
/// order, except that each run of adjacent 3D nodes draws back to front
/// ([`back_to_front`]); a flat node between two runs keeps them apart.
///
/// [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
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

/// The layer of node `i` of the canvas `area`, with its matte if it has
/// one, or `None` when it draws nothing.
fn matted(
    sources: Sources<'_>,
    at: Canvas,
    area: Rect,
    state: &FrameState,
    i: usize,
) -> Result<Option<Node>> {
    let list = &at.graph(sources.graph).nodes;
    let drawn = match blurs(&list[i]) {
        true => blurred(sources, at, area, state, i)?,
        false => layer(sources, &list[i], area, &state.view, &state.nodes[i])?,
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
    // matte; nothing outside its span.
    for (effect, input) in drawn.effects.iter_mut().zip(&list[i].inputs) {
        let Some(source) = *input else {
            continue;
        };
        let read = match state.nodes[source].shown {
            true => layer(
                sources,
                &list[source],
                area,
                &state.view,
                &state.nodes[source],
            )?,
            false => None,
        };
        let read = match read {
            Some(Node::Layer(read)) => Layer {
                transform: Affine::IDENTITY,
                projection: None,
                opacity: 1.0,
                ..read
            },
            _ => Layer::new(Vec::new()),
        };
        *effect = effect.clone().with_input(Node::Layer(read));
    }
    let Some((source, mode)) = list[i].matte else {
        return Ok(Some(Node::Layer(drawn)));
    };
    // The matte layer draws into the mask whether or not it is hidden, and
    // nothing outside its span; it may be matted in turn.
    let matte = &state.nodes[source];
    let source = match matte.shown {
        true => matted(sources, at, area, state, source)?,
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
        let routes = motion_scene::routes_from(sources.graph, list, source);
        let canvas = crate::mix::Canvas {
            nodes: list,
            fps: state.fps,
        };
        let sound = |route: &motion_scene::Route| {
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

/// How many times its shutter's samples a layer drawn once and moved may
/// take: each is a pass over the frame's pixels, so fewer than a drawing
/// made at every moment.
const MOVED_GROWTH: u32 = 2;

/// Node `i` of the canvas `at` blurred along its motion ([D57]), over the
/// canvas's open shutter. Its corners' travel over the shutter, in px,
/// decides the number of moments `n`: the shutter's samples, up to
/// [`Shutter::MAX_GROWTH`] times as many. Whether it draws is decided at
/// the frame: a moment outside its span still moves it, with its drawing
/// at the frame, so a layer does not fade in or out at its ends.
///
/// - A shape, text, rect, image or video layer without effects is drawn
///   at each moment, at `1/n` of its opacity and added, in a group of its
///   own, so what moves inside it blurs too.
/// - A `comp` layer, or one with effects, is drawn once at the frame and
///   moved to where each moment puts it (`Layer::motion`), up to
///   [`MOVED_GROWTH`] times its samples. Its content is its composition's,
///   which blurs by its own shutter, as a nested composition's does in the
///   projects the import reads.
///
/// [D57]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter
fn blurred(
    sources: Sources<'_>,
    at: Canvas,
    area: Rect,
    state: &FrameState,
    i: usize,
) -> Result<Option<Node>> {
    let graph = sources.graph;
    let comp = at.graph(graph);
    let node = &comp.nodes[i];
    let shutter = comp.shutter;
    if !shutter.open() {
        return layer(sources, node, area, &state.view, &state.nodes[i]);
    }
    let moment = |frame: f64| sources.moments.at(graph, at, frame);
    let opens = moment(state.frame + shutter.phase / 360.0);
    let closes = moment(state.frame + (shutter.phase + shutter.angle) / 360.0);
    let frame_node = &state.nodes[i];
    let moved = matches!(frame_node.content, ContentState::Comp(..)) || !frame_node.fx.is_empty();
    let growth = match moved {
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
        let Some(Node::Layer(drawn)) = layer(sources, node, area, &state.view, frame_node)? else {
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
        if let Some(Node::Layer(drawn)) = layer(sources, node, area, &at_moment.view, sampled)? {
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
            let p = motion_raster::kurbo::Point::new(x, y);
            match node.plane {
                None => node.transform * p,
                Some(plane) => space::project(plane, p).unwrap_or(
                    motion_raster::kurbo::Point::new(f64::INFINITY, f64::INFINITY),
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

/// The layer of one node of the canvas `area`, compiled as `compiled`,
/// without its matte, or `None` when it draws nothing.
fn layer(
    sources: Sources<'_>,
    compiled: &GraphNode,
    area: Rect,
    view: &View,
    node: &NodeState,
) -> Result<Option<Node>> {
    let opacity = node.number(Prop::OPACITY) as f32;
    if opacity <= 0.0 {
        return Ok(None);
    }
    let mut clip = None;
    let mut isolate = false;
    let mut bounds = None;
    let children = match &node.content {
        ContentState::Rect(shape) => {
            bounds = Some(Rect::new(0.0, 0.0, shape.width, shape.height));
            vec![rect(shape, node.fill)]
        }
        ContentState::Shapes(draws) => shape::draws(draws),
        ContentState::Text { text, glyphs } => text::draws(text, glyphs),
        ContentState::Image(content) => {
            bounds = Some(content.frame);
            vec![image(content, sources.pictures).map_err(|e| e.under(Place::of_layer(&node.id)))?]
        }
        ContentState::Video { video, seconds } => {
            bounds = Some(video.frame);
            let frame = (sources.clips)(&video.path, *seconds)
                .map_err(|e| e.at(VIDEO).under(Place::of_layer(&node.id)))?;
            frame
                .map(|picture| {
                    Node::Image(ImageDraw {
                        picture,
                        rect: video.frame,
                    })
                })
                .into_iter()
                .collect()
        }
        ContentState::Comp(index, Some(inner)) if compiled.collapse => {
            let comp = &sources.graph.comps[*index];
            let inner = joined(inner, view, node);
            let children = nodes(sources, Canvas::Comp(*index), canvas(comp), &inner)?;
            if children.is_empty() {
                return Ok(None);
            }
            let effects =
                effect::effects(&node.fx).map_err(|e| e.under(Place::of_layer(&node.id)))?;
            // Its own blend modes reach what is below unless its effects
            // need its drawing on its own, or an adjusting layer inside
            // needs only its composition below it.
            let isolate = !effects.is_empty() || comp.nodes.iter().any(|n| n.adjust);
            return Ok(Some(Node::Layer(Layer {
                opacity: opacity.min(1.0),
                effects,
                isolate,
                bounds: Some(area),
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
            let mut children = Vec::new();
            if opaque_enough(comp.background) {
                children.push(fill(canvas.to_path(TOLERANCE), color(comp.background)));
            }
            children.extend(nodes(sources, Canvas::Comp(*index), canvas, inner)?);
            children
        }
        ContentState::Comp(_, None)
        | ContentState::Audio
        | ContentState::Pivot
        | ContentState::Camera => Vec::new(),
        ContentState::Filter => {
            let effects =
                effect::effects(&node.fx).map_err(|e| e.under(Place::of_layer(&node.id)))?;
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
    let effects = effect::effects(&node.fx).map_err(|e| e.under(Place::of_layer(&node.id)))?;
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
/// [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
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
/// [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
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

/// An image node: its picture stretched over its box. The picture's errors
/// are placed at `image`, and name the file.
fn image(content: &ImageContent, pictures: &Pictures<'_>) -> Result<Node> {
    let picture = pictures(&content.path).map_err(|e| e.at(IMAGE))?;
    Ok(Node::Image(ImageDraw {
        picture,
        rect: content.frame,
    }))
}

/// `path` filled with `color` by the non-zero rule.
fn fill(path: motion_raster::kurbo::BezPath, color: Color) -> Node {
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
