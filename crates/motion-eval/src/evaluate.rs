//! Evaluating a scene graph at a point in time.
//!
//! Part of step 3 (Evaluate): turns the static graph from the compile step
//! into the concrete values of a single frame.

use crate::{FrameState, NodeState};
use motion_format::property::PROPERTIES;
use motion_scene::{Node, SceneGraph};
use tracing::instrument;

/// Evaluates every node of `graph` at `frame`.
///
/// `frame` is measured in frames from the start of the scene and may be
/// fractional: sub-frame times are allowed, for example to sample several
/// instants inside one frame for motion blur later.
// A `trace` span: per frame, and only a cached level check when disabled.
#[instrument(level = "trace", skip(graph))]
pub fn evaluate(graph: &SceneGraph, frame: f64) -> FrameState {
    FrameState {
        frame,
        nodes: graph.nodes.iter().map(|n| node_state(n, frame)).collect(),
    }
}

/// Resolves one node at `frame`: every track sampled, then clamped.
fn node_state(node: &Node, frame: f64) -> NodeState {
    // `zip` would silently drop a missing track, so check the invariant.
    debug_assert_eq!(node.tracks.len(), PROPERTIES.len());
    let values = PROPERTIES
        .iter()
        .zip(&node.tracks)
        .map(|(spec, track)| {
            let value = track.sample(frame);
            // A custom curve may overshoot so the motion springs past its
            // target; unbounded properties follow it, but a bounded one
            // (opacity) cannot leave its range.
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
