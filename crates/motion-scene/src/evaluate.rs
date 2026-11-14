//! Step 3 (Evaluate): every node's values at one frame.

use crate::{FrameState, Node, NodeState, SceneGraph};
use motion_format::property::PROPERTIES;
use tracing::instrument;

/// Evaluates every node of `graph` at `frame`, which may be fractional (sub-frame samples).
// `trace`: it runs per frame, and costs one cached level check when disabled.
#[instrument(level = "trace", skip(graph))]
pub fn evaluate(graph: &SceneGraph, frame: f64) -> FrameState {
    FrameState {
        frame,
        nodes: graph.nodes.iter().map(|n| node_state(n, frame)).collect(),
    }
}

/// One node at `frame`, each value clamped to its property's range.
fn node_state(node: &Node, frame: f64) -> NodeState {
    // `zip` would silently drop a missing track, so check the invariant.
    debug_assert_eq!(node.tracks.len(), PROPERTIES.len());
    let values = PROPERTIES
        .iter()
        .zip(&node.tracks)
        .map(|(spec, track)| {
            let value = track.sample(frame);
            // A custom curve may spring past its target; opacity must not.
            spec.range.map_or(value, |(lo, hi)| value.clamp(lo, hi))
        })
        .collect();
    NodeState {
        id: node.id.clone(),
        width: node.width,
        height: node.height,
        values,
    }
}

#[cfg(test)]
#[path = "../tests/unit/evaluate.rs"]
mod tests;
