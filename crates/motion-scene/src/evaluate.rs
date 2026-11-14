//! Step 3 (Evaluate): every node's values, transform and content at one
//! frame, compositions at their nodes' own time.

use kurbo::{Affine, Rect, Vec2};
use motion_format::Value;
use motion_format::property::{PROPERTIES, Prop, PropertySpec};
use tracing::instrument;

use crate::shape::{self, ShapeDraw};
use crate::{
    ContentState, FrameState, ItemNode, ItemState, Node, NodeContent, NodeState, PropertyTrack,
    SceneGraph,
};

/// Evaluates every node of `graph` at `frame`, which may be fractional (sub-frame samples).
// `trace`: it runs per frame, and costs one cached level check when disabled.
#[instrument(level = "trace", skip(graph))]
pub fn evaluate(graph: &SceneGraph, frame: f64) -> FrameState {
    evaluate_nodes(graph, &graph.nodes, frame, graph.fps)
}

/// Evaluates the composition at `index` of `graph.comps` at its own `frame`.
///
/// # Panics
/// If `index` is not a composition of `graph`.
pub fn evaluate_comp(graph: &SceneGraph, index: usize, frame: f64) -> FrameState {
    let comp = &graph.comps[index];
    evaluate_nodes(graph, &comp.nodes, frame, comp.fps)
}

/// The state of `nodes`, one canvas whose clock runs at `fps`, at `frame`.
fn evaluate_nodes(graph: &SceneGraph, nodes: &[Node], frame: f64, fps: f64) -> FrameState {
    let values: Vec<Vec<Value>> = nodes.iter().map(|n| values(n, frame)).collect();
    let mut world: Vec<Option<Affine>> = vec![None; nodes.len()];
    for i in 0..nodes.len() {
        transform(nodes, &values, &mut world, i);
    }
    let nodes = nodes
        .iter()
        .zip(values)
        .zip(world)
        .map(|((node, values), world)| {
            let local = node.timing.local(frame);
            let mut shown = node.timing.shows_at(frame);
            let content = match &node.content {
                NodeContent::Text { text, font } => ContentState::Text {
                    text: text.clone(),
                    font: font.clone(),
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
                    let state = (shown && !node.hidden)
                        .then(|| Box::new(evaluate_comp(graph, *index, inner)));
                    ContentState::Comp(*index, state)
                }
                NodeContent::Pivot => ContentState::Pivot,
            };
            let local_bounds = match &content {
                ContentState::Shapes(draws) => draws
                    .iter()
                    .map(ShapeDraw::bounds)
                    .reduce(|a, b| a.union(b))
                    .unwrap_or(Rect::ZERO),
                _ => Rect::new(0.0, 0.0, node.width, node.height),
            };
            NodeState {
                id: node.id.clone(),
                width: node.width,
                height: node.height,
                values,
                transform: world.expect("every transform is computed"),
                shown,
                hidden: node.hidden,
                content,
                local_bounds,
                fill: node.fill,
                fx: items_at(&node.fx, local),
            }
        })
        .collect();
    FrameState { frame, nodes }
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

/// Node `i`'s transform to the canvas, its parents' first. Validation rules
/// out parent cycles, so the recursion ends.
fn transform(
    nodes: &[Node],
    values: &[Vec<Value>],
    world: &mut [Option<Affine>],
    i: usize,
) -> Affine {
    if let Some(done) = world[i] {
        return done;
    }
    let parent = nodes[i]
        .parent
        .map_or(Affine::IDENTITY, |p| transform(nodes, values, world, p));
    let own = local_transform(&values[i]);
    world[i] = Some(parent * own);
    parent * own
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
