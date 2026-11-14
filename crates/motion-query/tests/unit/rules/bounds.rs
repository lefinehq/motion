//! Unit tests for `src/rules/bounds.rs`.

use super::*;
use motion_format::property::Prop;
use rstest::rstest;

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
    rotated(x, y, w, h, opacity, 1.0, 0.0)
}

/// Like `node`, with a scale and a rotation (degrees).
fn rotated(x: f64, y: f64, w: f64, h: f64, opacity: f64, scale: f64, rotate: f64) -> NodeState {
    let values = Prop::all()
        .map(|p| match p {
            Prop::X => x,
            Prop::Y => y,
            Prop::OPACITY => opacity,
            Prop::SCALE => scale,
            Prop::ROTATE => rotate,
            _ => p.spec().default,
        })
        .collect();
    NodeState {
        id: "n".into(),
        width: w,
        height: h,
        values,
    }
}

/// How far `node` leaves the canvas, or `None` when it is fine.
#[rstest]
#[case::inside_the_canvas(node(500.0, 250.0, 100.0, 100.0, 1.0), None)]
#[case::touching_the_canvas_edge(node(50.0, 250.0, 100.0, 100.0, 1.0), None)]
// 30 px past the right edge, 10 px past the top.
#[case::reports_the_largest_axis(node(980.0, 40.0, 100.0, 100.0, 1.0), Some(30.0))]
#[case::single_axis_overshoot(node(500.0, -10.0, 100.0, 100.0, 1.0), Some(60.0))]
// 2000 wide: no horizontal measurement, but 10 px past the top is found.
#[case::oversize_axis_is_not_measured(node(500.0, 40.0, 2000.0, 100.0, 1.0), Some(10.0))]
// Larger than the canvas on both axes: nothing to report at all.
#[case::oversize_on_both_axes(node(0.0, 0.0, 2000.0, 900.0, 1.0), None)]
#[case::full_bleed(node(500.0, 250.0, 1000.0, 500.0, 1.0), None)]
#[case::invisible(node(5000.0, 0.0, 100.0, 100.0, 0.0), None)]
#[case::float_noise(node(500.0, 250.0 + 1e-12, 1000.0, 500.0, 1.0), None)]
// 100 wide at scale 2 is 200 wide: 100 px from centre 50 reaches -50.
#[case::scale_grows_the_box(rotated(50.0, 250.0, 100.0, 100.0, 1.0, 2.0, 0.0), Some(50.0))]
// Unrotated, this box (100 px half-width) pokes out by 50 px; a quarter turn
// makes it 10 px: rotation changes the verdict.
#[case::a_quarter_turn(rotated(50.0, 250.0, 200.0, 20.0, 1.0, 1.0, 90.0), None)]
fn offscreen_amount(#[case] node: NodeState, #[case] amount: Option<f64>) {
    assert_eq!(offscreen(&node, &graph()).map(|o| o.amount), amount);
}

/// How far `node` leaves the safe area, x 50..950 and y 25..475.
#[rstest]
#[case::full_bleed_is_never_unsafe(node(500.0, 250.0, 1000.0, 500.0, 1.0), None)]
#[case::invisible(node(5000.0, 0.0, 100.0, 100.0, 0.0), None)]
// The left edge at 40 is 10 px out.
#[case::five_percent_per_side(node(90.0, 250.0, 100.0, 100.0, 1.0), Some(10.0))]
#[case::on_the_safe_edge(node(100.0, 250.0, 100.0, 100.0, 1.0), None)]
fn outside_safe_area_amount(#[case] node: NodeState, #[case] amount: Option<f64>) {
    assert_eq!(outside_safe_area(&node, &graph()).map(|o| o.amount), amount);
}

#[test]
fn offscreen_aims_at_the_safe_area_and_unsafe_at_itself() {
    let (area, aim) = (OFFSCREEN.areas)(&graph());
    assert_eq!(
        (area, aim),
        (
            Rect::new(0.0, 0.0, 1000.0, 500.0),
            Rect::new(50.0, 25.0, 950.0, 475.0)
        )
    );
    let (area, aim) = (UNSAFE.areas)(&graph());
    assert_eq!(area, aim);
}

#[test]
fn a_rotated_box_leaving_the_canvas_is_caught() {
    // 200x20 at 45 degrees reaches about 77.8 px from its centre; the centre
    // is 50 px from the left edge, so the box pokes out by about 27.8 px.
    let o = offscreen(&rotated(50.0, 250.0, 200.0, 20.0, 1.0, 1.0, 45.0), &graph()).unwrap();
    assert!((o.amount - 27.78).abs() < 0.01, "{}", o.amount);
}
