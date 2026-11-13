//! Evaluating a scene graph at a point in time.
//!
//! Part of step 3 (Evaluate): turns the static graph from the compile step
//! into the concrete values of a single frame.

use crate::{FrameState, NodeState};
use motion_format::property::{PROPERTIES, property};
use motion_scene::{Node, SceneGraph};

/// Evaluates every node of `graph` at `frame`.
///
/// `frame` is measured in frames from the start of the scene and may be
/// fractional: sub-frame times are allowed, for example to sample several
/// instants inside one frame for motion blur later.
pub fn evaluate(graph: &SceneGraph, frame: f64) -> FrameState {
    FrameState {
        frame,
        nodes: graph.nodes.iter().map(|n| node_state(n, frame)).collect(),
    }
}

/// The index of the property called `name` in the registry.
fn index_of(name: &str) -> usize {
    property(name)
        .map(|(i, _)| i)
        .expect("the registry holds the property NodeState reports")
}

/// Samples the track of the property called `name` at `frame`.
fn sample(node: &Node, name: &str, frame: f64) -> f64 {
    node.tracks[index_of(name)].sample(frame)
}

/// Resolves one node at `frame`.
fn node_state(node: &Node, frame: f64) -> NodeState {
    let opacity = index_of("opacity");
    // A custom curve may overshoot to make the motion spring past its
    // target; position follows it, but opacity cannot leave its range.
    let (lo, hi) = PROPERTIES[opacity].range.unwrap_or((f64::MIN, f64::MAX));
    NodeState {
        id: node.id.clone(),
        x: sample(node, "x", frame),
        y: sample(node, "y", frame),
        width: node.width,
        height: node.height,
        opacity: sample(node, "opacity", frame).clamp(lo, hi),
    }
}

#[cfg(test)]
#[path = "../tests/unit/evaluate.rs"]
mod tests;
