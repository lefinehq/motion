//! Evaluating a scene graph at a point in time.
//!
//! Part of step 3 (Evaluate): turns the static graph from the compile step
//! into the concrete values of a single frame.

use crate::{FrameState, NodeState};
use motion_scene::{Node, SceneGraph, curve};

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

/// Resolves one node at `frame`.
fn node_state(node: &Node, frame: f64) -> NodeState {
    // Nodes without an entrance are always fully visible and in place.
    let (k, dx, dy) = match node.enter {
        None => (1.0, 0.0, 0.0),
        Some(enter) => {
            let p = if frame < enter.start {
                0.0
            } else if enter.duration <= 0.0 {
                // A zero-length entrance would divide by zero: it simply
                // jumps to its end at its start frame.
                1.0
            } else {
                (frame - enter.start) / enter.duration
            };
            let k = curve::sample(enter.curve, p);
            // The offset shrinks to nothing as the entrance completes.
            (
                k,
                enter.from_offset.0 * (1.0 - k),
                enter.from_offset.1 * (1.0 - k),
            )
        }
    };
    NodeState {
        id: node.id.clone(),
        x: node.x + dx,
        y: node.y + dy,
        width: node.width,
        height: node.height,
        // A custom curve may overshoot (k > 1) to make the motion spring past
        // its target; position follows it, but opacity cannot exceed fully
        // opaque or go below invisible.
        opacity: k.clamp(0.0, 1.0),
    }
}

#[cfg(test)]
#[path = "../tests/unit/evaluate.rs"]
mod tests;
