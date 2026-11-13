//! Rules about a node's box leaving the canvas.
//!
//! Part of step 4 (Query): `offscreen` for the canvas itself, `unsafe` for
//! the title-safe area inside it.

use super::{Overshoot, Rule, overshoot};
use crate::report::Severity;
use motion_eval::NodeState;
use motion_scene::SceneGraph;

/// The title-safe area keeps this fraction of the canvas free on each side.
const SAFE_INSET: f64 = 0.05;

/// An opacity at or below this counts as invisible.
const INVISIBLE: f64 = 0.0;

/// A visible node must lie inside the canvas.
pub(super) const OFFSCREEN: Rule = Rule {
    code: "offscreen",
    severity: Severity::Error,
    suppressed_by: None,
    evaluate: offscreen,
};

/// A visible node should lie inside the title-safe area.
pub(super) const UNSAFE: Rule = Rule {
    code: "unsafe",
    severity: Severity::Warning,
    // An off-screen node is also outside the safe area; say it once.
    suppressed_by: Some("offscreen"),
    evaluate: outside_safe_area,
};

fn offscreen(node: &NodeState, graph: &SceneGraph) -> Option<Overshoot> {
    // Invisible nodes (before their entrance) may start off-screen.
    if node.opacity <= INVISIBLE {
        return None;
    }
    overshoot(node, (0.0, 0.0, graph.width, graph.height))
}

fn outside_safe_area(node: &NodeState, graph: &SceneGraph) -> Option<Overshoot> {
    if node.opacity <= INVISIBLE {
        return None;
    }
    let (dx, dy) = (graph.width * SAFE_INSET, graph.height * SAFE_INSET);
    overshoot(node, (dx, dy, graph.width - dx, graph.height - dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Edge;

    fn graph() -> SceneGraph {
        SceneGraph {
            width: 1000.0,
            height: 500.0,
            fps: 30.0,
            duration_frames: 30.0,
            nodes: vec![],
        }
    }

    fn node(x: f64, y: f64, w: f64, h: f64, opacity: f64) -> NodeState {
        NodeState {
            id: "n".into(),
            x,
            y,
            width: w,
            height: h,
            opacity,
        }
    }

    #[test]
    fn inside_the_canvas_is_fine() {
        assert_eq!(
            offscreen(&node(500.0, 250.0, 100.0, 100.0, 1.0), &graph()),
            None
        );
    }

    #[test]
    fn touching_the_canvas_edge_is_fine() {
        assert_eq!(
            offscreen(&node(50.0, 250.0, 100.0, 100.0, 1.0), &graph()),
            None
        );
    }

    #[test]
    fn reports_the_largest_edge() {
        // 30 px past the right edge, 10 px past the top.
        let o = offscreen(&node(980.0, 40.0, 100.0, 100.0, 1.0), &graph()).unwrap();
        assert_eq!((o.amount, o.edge), (30.0, Edge::Right));
        let o = offscreen(&node(500.0, -10.0, 100.0, 100.0, 1.0), &graph()).unwrap();
        assert_eq!((o.amount, o.edge), (60.0, Edge::Top));
    }

    #[test]
    fn invisible_nodes_are_ignored() {
        assert_eq!(
            offscreen(&node(5000.0, 0.0, 100.0, 100.0, 0.0), &graph()),
            None
        );
        assert_eq!(
            outside_safe_area(&node(5000.0, 0.0, 100.0, 100.0, 0.0), &graph()),
            None
        );
    }

    #[test]
    fn safe_area_is_five_percent_per_side() {
        // Safe area is x 50..950, y 25..475. Left edge at 40 is 10 px out.
        let o = outside_safe_area(&node(90.0, 250.0, 100.0, 100.0, 1.0), &graph()).unwrap();
        assert_eq!((o.amount, o.edge), (10.0, Edge::Left));
        assert_eq!(
            outside_safe_area(&node(100.0, 250.0, 100.0, 100.0, 1.0), &graph()),
            None
        );
    }

    #[test]
    fn float_noise_is_not_an_overshoot() {
        assert_eq!(
            offscreen(&node(500.0, 250.0 + 1e-12, 1000.0, 500.0, 1.0), &graph()),
            None
        );
    }
}
