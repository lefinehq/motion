//! Unit tests for `src/rules/bounds.rs`.

use super::*;
use motion_format::Value;
use motion_format::property::Prop;
use motion_scene::local_transform;

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
    let values: Vec<Value> = Prop::all()
        .map(|p| match p {
            Prop::POSITION => Value::Point([x, y]),
            Prop::ANCHOR => Value::Point([w / 2.0, h / 2.0]),
            Prop::SCALE => Value::Point([scale, scale]),
            Prop::OPACITY => Value::Number(opacity),
            Prop::ROTATE => Value::Number(rotate),
            _ => unreachable!("every property is listed"),
        })
        .collect();
    NodeState {
        id: "n".into(),
        width: w,
        height: h,
        transform: local_transform(&values),
        values,
        shown: true,
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
fn reports_the_largest_axis() {
    // 30 px past the right edge, 10 px past the top.
    let o = offscreen(&node(980.0, 40.0, 100.0, 100.0, 1.0), &graph()).unwrap();
    assert_eq!(o.amount, 30.0);
}

#[test]
fn single_axis_overshoot() {
    let o = offscreen(&node(500.0, -10.0, 100.0, 100.0, 1.0), &graph()).unwrap();
    assert_eq!(o.amount, 60.0);
}

#[test]
fn oversize_axis_is_not_measured() {
    // 2000 wide: no horizontal measurement, but 10 px past the top is found.
    let o = offscreen(&node(500.0, 40.0, 2000.0, 100.0, 1.0), &graph()).unwrap();
    assert_eq!(o.amount, 10.0);
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
fn offscreen_aims_at_the_safe_area_and_unsafe_at_itself() {
    let (area, aim) = (OFFSCREEN.areas)(&graph());
    assert_eq!(
        (area, aim),
        ((0.0, 0.0, 1000.0, 500.0), (50.0, 25.0, 950.0, 475.0))
    );
    let (area, aim) = (UNSAFE.areas)(&graph());
    assert_eq!(area, aim);
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
    assert_eq!(o.amount, 10.0);
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

#[test]
fn a_rotated_box_leaving_the_canvas_is_caught() {
    // 200x20 at 45 degrees reaches about 77.8 px from its centre; the centre
    // is 50 px from the left edge, so the box pokes out by about 27.8 px.
    let o = offscreen(&rotated(50.0, 250.0, 200.0, 20.0, 1.0, 1.0, 45.0), &graph()).unwrap();
    assert!((o.amount - 27.78).abs() < 0.01, "{}", o.amount);
    // Unrotated, the same box (100 px half-width) pokes out by 50 px, and a
    // quarter turn makes it 10 px: rotation changes the verdict.
    let upright = offscreen(&rotated(50.0, 250.0, 200.0, 20.0, 1.0, 1.0, 90.0), &graph());
    assert_eq!(upright, None);
}

#[test]
fn scale_grows_the_box_the_rules_measure() {
    // 100 wide at scale 2 is 200 wide: 100 px from centre 50 reaches -50.
    let o = offscreen(&rotated(50.0, 250.0, 100.0, 100.0, 1.0, 2.0, 0.0), &graph()).unwrap();
    assert_eq!(o.amount, 50.0);
}

#[test]
fn a_node_outside_its_span_is_not_checked() {
    let hidden = NodeState {
        shown: false,
        ..node(-500.0, 250.0, 100.0, 100.0, 1.0)
    };
    assert_eq!(offscreen(&hidden, &graph()), None);
    assert!(offscreen(&node(-500.0, 250.0, 100.0, 100.0, 1.0), &graph()).is_some());
}
