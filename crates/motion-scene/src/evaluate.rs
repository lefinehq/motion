//! Step 3 (Evaluate): every node's values and transform at one frame.

use kurbo::{Affine, Vec2};
use motion_format::Value;
use motion_format::property::{PROPERTIES, Prop};
use tracing::instrument;

use crate::{FrameState, Node, NodeState, SceneGraph};

/// Evaluates every node of `graph` at `frame`, which may be fractional (sub-frame samples).
// `trace`: it runs per frame, and costs one cached level check when disabled.
#[instrument(level = "trace", skip(graph))]
pub fn evaluate(graph: &SceneGraph, frame: f64) -> FrameState {
    let values: Vec<Vec<Value>> = graph.nodes.iter().map(|n| values(n, frame)).collect();
    let mut world: Vec<Option<Affine>> = vec![None; graph.nodes.len()];
    for i in 0..graph.nodes.len() {
        transform(graph, &values, &mut world, i);
    }
    let nodes = graph
        .nodes
        .iter()
        .zip(values)
        .zip(world)
        .map(|((node, values), world)| NodeState {
            id: node.id.clone(),
            width: node.width,
            height: node.height,
            values,
            transform: world.expect("every transform is computed"),
            shown: node.timing.shows_at(frame),
        })
        .collect();
    FrameState { frame, nodes }
}

/// One node's values at the parent's `frame`, sampled at its own time and
/// each component clamped to its property's range.
fn values(node: &Node, frame: f64) -> Vec<Value> {
    // `zip` would silently drop a missing track, so check the invariant.
    debug_assert_eq!(node.tracks.len(), PROPERTIES.len());
    let local = node.timing.local(frame);
    PROPERTIES
        .iter()
        .zip(&node.tracks)
        .map(|(spec, track)| {
            let value = track.sample(local, spec.ty);
            // A custom curve may spring past its target; opacity must not.
            match spec.range {
                None => value,
                Some((lo, hi)) => {
                    let clamped: Vec<f64> =
                        value.components().iter().map(|v| v.clamp(lo, hi)).collect();
                    Value::from_components(spec.ty, &clamped)
                }
            }
        })
        .collect()
}

/// Node `i`'s transform to the canvas, its parents' first. Validation rules
/// out parent cycles, so the recursion ends.
fn transform(
    graph: &SceneGraph,
    values: &[Vec<Value>],
    world: &mut [Option<Affine>],
    i: usize,
) -> Affine {
    if let Some(done) = world[i] {
        return done;
    }
    let parent = graph.nodes[i]
        .parent
        .map_or(Affine::IDENTITY, |p| transform(graph, values, world, p));
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
