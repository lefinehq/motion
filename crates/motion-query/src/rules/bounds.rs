//! Rules about a node's box leaving the canvas.
//!
//! A side branch working on the output of step 3 (not a numbered rendering
//! step): `offscreen` for the canvas itself, `unsafe` for the title-safe area
//! inside it. Both measure only the axes on which the box fits their area, so
//! a full-bleed background is never reported (see the module docs of
//! `rules`).

use super::{Area, Overshoot, Rule, overshoot};
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

/// The title-safe area of the canvas.
fn safe_area(graph: &SceneGraph) -> Area {
    let (dx, dy) = (graph.width * SAFE_INSET, graph.height * SAFE_INSET);
    (dx, dy, graph.width - dx, graph.height - dy)
}

fn offscreen(node: &NodeState, graph: &SceneGraph) -> Option<Overshoot> {
    // Invisible nodes (before their entrance) may start off-screen.
    if node.opacity <= INVISIBLE {
        return None;
    }
    let canvas = (0.0, 0.0, graph.width, graph.height);
    // Aim the fix at the safe area so one edit clears both rules; `overshoot`
    // falls back to the canvas on an axis where the box does not fit it.
    overshoot(node, canvas, safe_area(graph))
}

fn outside_safe_area(node: &NodeState, graph: &SceneGraph) -> Option<Overshoot> {
    if node.opacity <= INVISIBLE {
        return None;
    }
    let safe = safe_area(graph);
    overshoot(node, safe, safe)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn reports_the_largest_axis_and_aims_at_the_safe_area() {
        // 30 px past the right edge, 10 px past the top: both axes move at once.
        // Safe x range for the centre is 100..900 (box is 100 wide, area 50..950),
        // safe y range is 75..425.
        let o = offscreen(&node(980.0, 40.0, 100.0, 100.0, 1.0), &graph()).unwrap();
        assert_eq!((o.amount, o.shift), (30.0, (-80.0, 35.0)));
    }

    #[test]
    fn single_axis_overshoot_moves_only_that_axis() {
        let o = offscreen(&node(500.0, -10.0, 100.0, 100.0, 1.0), &graph()).unwrap();
        assert_eq!((o.amount, o.shift), (60.0, (0.0, 85.0)));
    }

    #[test]
    fn oversize_axis_is_not_measured() {
        // 2000 wide: no horizontal measurement, but 10 px past the top is found.
        let o = offscreen(&node(500.0, 40.0, 2000.0, 100.0, 1.0), &graph()).unwrap();
        assert_eq!((o.amount, o.shift), (10.0, (0.0, 35.0)));
        // Larger than the canvas on both axes: nothing to report at all.
        assert_eq!(
            offscreen(&node(0.0, 0.0, 2000.0, 900.0, 1.0), &graph()),
            None
        );
    }

    #[test]
    fn full_bleed_is_never_unsafe() {
        let full = node(500.0, 250.0, 1000.0, 500.0, 1.0);
        assert_eq!(offscreen(&full, &graph()), None);
        assert_eq!(outside_safe_area(&full, &graph()), None);
    }

    #[test]
    fn box_too_big_for_the_safe_area_aims_at_the_canvas() {
        // 980 wide fits the canvas but not the safe area (900): fall back to 0..1000.
        let o = offscreen(&node(520.0, 250.0, 980.0, 100.0, 1.0), &graph()).unwrap();
        assert_eq!(o.shift, (-10.0, 0.0));
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
        assert_eq!((o.amount, o.shift), (10.0, (10.0, 0.0)));
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
