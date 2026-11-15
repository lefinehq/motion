//! Step 3 (Evaluate): every node's values, transform and content at one
//! frame, compositions at their nodes' own time.

use std::sync::Arc;

use kurbo::{Affine, Rect, Vec2};
use motion_format::Value;
use motion_format::property::{PROPERTIES, Prop, PropertySpec};
use tracing::instrument;

use crate::animate::{self, AnimatorState, COUNTS, Parts, Turns};
use crate::shape::{self, ShapeDraw};
use crate::space::{self, View};
use crate::{
    CompGraph, ContentState, FrameState, ItemNode, ItemState, Node, NodeContent, NodeState,
    PropertyTrack, SceneGraph,
};
use motion_format::item::props;

/// Evaluates every node of `graph` at `frame`, which may be fractional (sub-frame samples).
// `trace`: it runs per frame, and costs one cached level check when disabled.
#[instrument(level = "trace", skip(graph))]
pub fn evaluate(graph: &SceneGraph, frame: f64) -> FrameState {
    evaluate_nodes(graph, &graph.root, frame)
}

/// Evaluates the composition at `index` of `graph.comps` at its own `frame`.
///
/// # Panics
/// If `index` is not a composition of `graph`.
pub fn evaluate_comp(graph: &SceneGraph, index: usize, frame: f64) -> FrameState {
    evaluate_nodes(graph, &graph.comps[index], frame)
}

/// The state of the nodes of `comp`, one canvas, at `frame` of its clock.
fn evaluate_nodes(graph: &SceneGraph, comp: &CompGraph, frame: f64) -> FrameState {
    let (nodes, fps) = (comp.nodes.as_slice(), comp.fps);
    let values: Vec<Vec<Value>> = nodes.iter().map(|n| values(n, frame)).collect();
    let mut world: Vec<Option<Placed>> = vec![None; nodes.len()];
    for i in 0..nodes.len() {
        place(nodes, &values, &mut world, i);
    }
    let view = view(comp, &values, &world, frame);
    // A hidden node another uses as its matte, or whose drawing another's
    // effect reads, is still drawn, into the mask or for the effect.
    let mattes: Vec<bool> = (0..nodes.len())
        .map(|i| {
            nodes.iter().any(|n| {
                n.matte.is_some_and(|(source, _)| source == i) || n.inputs.contains(&Some(i))
            })
        })
        .collect();
    let nodes = nodes
        .iter()
        .zip(values)
        .zip(world)
        .zip(mattes)
        .map(|(((node, values), world), matte)| {
            let local = node.timing.local(frame);
            let mut shown = node.timing.shows_at(frame);
            let content = match &node.content {
                NodeContent::Text { text, animators } => ContentState::Text {
                    text: text.clone(),
                    glyphs: match animators.is_empty() {
                        true => Vec::new(),
                        false => {
                            let animators: Vec<AnimatorState> = animators
                                .iter()
                                .zip(items_at(animators, local))
                                .map(|(node, item)| AnimatorState {
                                    turns: turns(node, &item, &text.parts, local),
                                    item,
                                    applies: node.written.clone(),
                                })
                                .collect();
                            animate::glyph_states(text, &text.parts, &animators)
                        }
                    },
                },
                NodeContent::Rect(r) => ContentState::Rect(*r),
                NodeContent::Shapes(items) => {
                    ContentState::Shapes(shape::draws(&items_at(items, local)))
                }
                NodeContent::Comp(index) => {
                    let comp = &graph.comps[*index];
                    // The node's own time, in seconds, is the composition's.
                    let inner = local * comp.fps / fps;
                    shown &= (0.0..comp.duration_frames).contains(&inner);
                    let state = (shown && (!node.hidden || matte))
                        .then(|| Box::new(evaluate_comp(graph, *index, inner)));
                    ContentState::Comp(*index, state)
                }
                NodeContent::Image(image) => ContentState::Image(Arc::clone(image)),
                NodeContent::Video(video) => ContentState::Video {
                    video: Arc::clone(video),
                    seconds: local / fps,
                },
                NodeContent::Audio(_) => ContentState::Audio,
                NodeContent::Pivot => ContentState::Pivot,
                NodeContent::Camera { .. } => ContentState::Camera,
                NodeContent::Filter => ContentState::Filter,
            };
            let local_bounds = match &content {
                ContentState::Text { text, glyphs } if !glyphs.is_empty() => {
                    animate::bounds(text, glyphs)
                }
                ContentState::Shapes(draws) => draws
                    .iter()
                    .map(ShapeDraw::bounds)
                    .reduce(|a, b| a.union(b))
                    .unwrap_or(Rect::ZERO),
                _ => node.bounds,
            };
            let world = world.expect("every transform is computed");
            NodeState {
                id: node.id.clone(),
                values,
                transform: world.flat,
                space: world.space,
                plane: match &node.content {
                    NodeContent::Camera { .. } => None,
                    _ => world.space.map(|space| view.plane(space)),
                },
                shown,
                hidden: node.hidden,
                content,
                own_box: node.bounds,
                local_bounds,
                fill: node.fill,
                mask: items_at(&node.mask, local),
                fx: items_at(&node.fx, local),
                seconds: local / fps,
            }
        })
        .collect();
    FrameState {
        frame,
        fps,
        nodes,
        view,
    }
}

/// The camera `comp` is seen through at `frame`: its last camera node that
/// shows, or the default camera.
fn view(comp: &CompGraph, values: &[Vec<Value>], world: &[Option<Placed>], frame: f64) -> View {
    comp.nodes
        .iter()
        .enumerate()
        .rev()
        .find(|(_, node)| {
            matches!(node.content, NodeContent::Camera { .. })
                && !node.hidden
                && node.timing.shows_at(frame)
        })
        .map_or_else(
            || View::default_for(comp.width, comp.height),
            |(i, _)| {
                let zoom = values[i][Prop::ZOOM.index()]
                    .as_number()
                    .expect("a number property");
                let space = world[i]
                    .and_then(|placed| placed.space)
                    .expect("a camera is placed in space");
                View::camera(space, zoom, comp.width, comp.height)
            },
        )
}

/// Items at a node's own frame `local`.
fn items_at(items: &[ItemNode], local: f64) -> Vec<ItemState> {
    items
        .iter()
        .map(|item| ItemState {
            kind: item.kind,
            id: item.id.clone(),
            values: sample(item.kind.registry.rows, &item.tracks, local),
            settings: item.settings.clone(),
            items: items_at(&item.items, local),
        })
        .collect()
}

/// A staggered animator as each part sees it at `local`: as it was each
/// part's wait earlier. The stagger, order and seed are read at `local`.
fn turns(node: &ItemNode, item: &ItemState, parts: &Parts, local: f64) -> Option<Turns> {
    let stagger = item.number(props::ANIMATOR_STAGGER);
    if stagger <= 0.0 {
        return None;
    }
    let by = COUNTS
        .iter()
        .position(|c| *c == item.setting("by"))
        .expect("every count has a row");
    let waits = animate::waits(
        item.setting("order"),
        item.number(props::ANIMATOR_SEED),
        parts.counts[by],
    );
    let parts = waits
        .into_iter()
        .map(|wait| {
            let mut seen = items_at(std::slice::from_ref(node), local - wait * stagger);
            seen.pop().expect("one item in, one out")
        })
        .collect();
    Some(Turns { by, parts })
}

/// One node's values at the parent's `frame`, sampled at its own time.
fn values(node: &Node, frame: f64) -> Vec<Value> {
    sample(PROPERTIES, &node.tracks, node.timing.local(frame))
}

/// The value of each of `rows` from its track at `local`, each component
/// clamped to its row's range.
fn sample(rows: &[PropertySpec], tracks: &[PropertyTrack], local: f64) -> Vec<Value> {
    // `zip` would silently drop a missing track, so check the invariant.
    debug_assert_eq!(tracks.len(), rows.len());
    rows.iter()
        .zip(tracks)
        .map(|(spec, track)| {
            let value = track.sample(local, spec.ty);
            // A custom curve may spring past its target; opacity must not.
            match spec.range {
                None => value,
                Some((lo, hi)) => {
                    let clamped: Vec<f64> =
                        value.components().iter().map(|v| v.clamp(lo, hi)).collect();
                    value.with_components(&clamped)
                }
            }
        })
        .collect()
}

/// Where a node is: flat on the canvas and, for a 3D node, in space.
#[derive(Debug, Clone, Copy)]
struct Placed {
    /// To the canvas; a 3D node's plane seen straight on.
    flat: Affine,
    /// To the composition's space, for a 3D node or camera.
    space: Option<glam::DMat4>,
}

/// Node `i`'s place, its parents' first. Validation rules out parent
/// cycles, so the recursion ends.
///
/// A 3D node under a flat parent moves with the parent's transform lifted
/// into space; a flat node under a 3D parent moves with the parent seen
/// straight on, without its depth.
fn place(nodes: &[Node], values: &[Vec<Value>], world: &mut [Option<Placed>], i: usize) -> Placed {
    if let Some(done) = world[i] {
        return done;
    }
    let parent = nodes[i].parent.map(|p| place(nodes, values, world, p));
    let node = &nodes[i];
    let placed = match (node.three_d, &node.content) {
        (true, content) => {
            let own = match content {
                NodeContent::Camera { aim } => space::camera_matrix(&values[i], *aim),
                _ => space::local_matrix(&values[i]),
            };
            let above = parent.map_or(glam::DMat4::IDENTITY, |p| {
                p.space.unwrap_or_else(|| space::lift(p.flat))
            });
            let matrix = above * own;
            Placed {
                flat: space::flatten(matrix),
                space: Some(matrix),
            }
        }
        (false, _) => {
            let above = parent.map_or(Affine::IDENTITY, |p| p.space.map_or(p.flat, space::flatten));
            Placed {
                flat: above * local_transform(&values[i]),
                space: None,
            }
        }
    };
    world[i] = Some(placed);
    placed
}

/// A node's own transform from its values, one per property in registry
/// order: move its anchor to the origin, scale, turn clockwise, then move the
/// anchor to its position.
///
/// # Panics
/// If `values` does not follow the registry's types.
pub fn local_transform(values: &[Value]) -> Affine {
    let point = |prop: Prop| {
        let [x, y] = values[prop.index()].as_point().expect("a point property");
        Vec2::new(x, y)
    };
    let rotate = values[Prop::ROTATE.index()]
        .as_number()
        .expect("a number property");
    let scale = point(Prop::SCALE);
    Affine::translate(point(Prop::POSITION))
        * Affine::rotate(rotate.to_radians())
        * Affine::scale_non_uniform(scale.x, scale.y)
        * Affine::translate(-point(Prop::ANCHOR))
}

#[cfg(test)]
#[path = "../tests/unit/evaluate.rs"]
mod tests;
