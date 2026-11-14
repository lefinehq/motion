//! Unit tests for `src/evaluate.rs`.

use super::*;
use motion_format::property::Prop;

/// A node's opacity.
fn opacity(n: &NodeState) -> f64 {
    n.number(Prop::OPACITY)
}

/// A node's position on one axis.
fn x(n: &NodeState) -> f64 {
    n.point(Prop::POSITION)[0]
}

fn y(n: &NodeState) -> f64 {
    n.point(Prop::POSITION)[1]
}

fn graph(json: &str) -> SceneGraph {
    crate::compile(
        motion_format::parse_document(json).unwrap().scene(),
        &crate::ApproxTextMeasure,
    )
}

/// A 100x100 rect centred at y=700 with a linear 12-frame entrance from frame 15.
const LIN: &str = r#"{"motion":1,"dur":"2s","layers":[{"id":"c","rect":"100x100","at":"center 700","in":"fade-up 60px 12f linear @15f"}]}"#;

#[test]
fn hidden_and_offset_before_the_entrance() {
    let n = &evaluate(&graph(LIN), 0.0).nodes[0];
    assert_eq!((opacity(n), y(n)), (0.0, 760.0));
}

#[test]
fn interpolates_during_the_entrance() {
    let n = &evaluate(&graph(LIN), 21.0).nodes[0];
    assert_eq!((opacity(n), y(n)), (0.5, 730.0));
    assert_eq!(
        (n.top(), n.bottom(), n.left(), n.right()),
        (680.0, 780.0, 910.0, 1010.0)
    );
}

#[test]
fn settles_at_the_end_and_stays_there() {
    for f in [27.0, 59.0, 1000.0] {
        let n = &evaluate(&graph(LIN), f).nodes[0];
        assert_eq!((opacity(n), y(n)), (1.0, 700.0));
    }
}

#[test]
fn layers_without_entrance_are_always_visible() {
    let g = graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"}]}"#);
    assert_eq!(opacity(&evaluate(&g, 0.0).nodes[0]), 1.0);
}

#[test]
fn zero_duration_jumps_at_its_start_frame() {
    let g =
        graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10","in":"fade 0f @5f"}]}"#);
    assert_eq!(opacity(&evaluate(&g, 4.9).nodes[0]), 0.0);
    assert_eq!(opacity(&evaluate(&g, 5.0).nodes[0]), 1.0);
}

#[test]
fn sub_frame_times_are_supported() {
    assert!((opacity(&evaluate(&graph(LIN), 15.6).nodes[0]) - 0.05).abs() < 1e-9);
}

#[test]
fn records_the_evaluated_frame_and_keeps_paint_order() {
    let g = graph(
        r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"},{"id":"b","rect":"20x20"}]}"#,
    );
    let s = evaluate(&g, 3.5);
    assert_eq!(s.frame, 3.5);
    let ids: Vec<_> = s.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
}

#[test]
fn an_overshooting_curve_moves_past_the_target_but_opacity_stays_at_one() {
    let json = r#"{"motion":1,"dur":"2s","layers":[{"id":"c","rect":"100x100","at":"center 700","in":"fade-up 60px 12f cubic(.3,0,.3,1.6) @0f"}]}"#;
    let g = graph(json);
    let past = (1..12)
        .map(|f| evaluate(&g, f as f64).nodes[0].clone())
        .find(|n| y(n) < 700.0)
        .expect("the curve overshoots above its resting place");
    assert_eq!(opacity(&past), 1.0);
    assert_eq!(y(&evaluate(&g, 12.0).nodes[0]), 700.0);
}

/// A 100x100 rect centred at (500, 400), with the preset under test.
fn preset(name: &str) -> SceneGraph {
    graph(&format!(
        r#"{{"motion":1,"dur":"2s","layers":[{{"id":"c","rect":"100x100","at":"500 400","in":"{name} 10f linear @0f"}}]}}"#
    ))
}

#[test]
fn pop_scales_up_while_fading_in() {
    let g = preset("pop");
    let n = &evaluate(&g, 5.0).nodes[0];
    assert!((opacity(n) - 0.5).abs() < 1e-9);
    assert!((n.point(Prop::SCALE)[0] - 0.8).abs() < 1e-9);
    // Position and rotation are untouched.
    assert_eq!((x(n), y(n), n.number(Prop::ROTATE)), (500.0, 400.0, 0.0));
    let done = &evaluate(&g, 10.0).nodes[0];
    assert_eq!((opacity(done), done.point(Prop::SCALE)), (1.0, [1.0, 1.0]));
}

#[test]
fn spin_in_rotates_while_fading_in() {
    let g = preset("spin-in");
    let n = &evaluate(&g, 5.0).nodes[0];
    assert!((opacity(n) - 0.5).abs() < 1e-9);
    assert!((n.number(Prop::ROTATE) + 45.0).abs() < 1e-9);
    assert_eq!(evaluate(&g, 10.0).nodes[0].number(Prop::ROTATE), 0.0);
}

#[test]
fn opacity_is_clamped_while_y_overshoots_on_the_same_curve() {
    let g = graph(
        r#"{"motion":1,"dur":"2s","layers":[{"id":"c","rect":"100x100","at":"center 700","in":"fade-up 60px 12f cubic(.3,0,.3,1.6) @0f"}]}"#,
    );
    let mut y_past = false;
    for f in 0..=12 {
        let n = &evaluate(&g, f as f64).nodes[0];
        assert!((0.0..=1.0).contains(&opacity(n)), "frame {f}");
        y_past |= y(n) < 700.0;
    }
    assert!(y_past, "y springs past its resting place");
}

#[test]
fn values_are_parallel_to_the_registry() {
    let n = &evaluate(&graph(LIN), 0.0).nodes[0];
    assert_eq!(n.values.len(), motion_format::property::PROPERTIES.len());
}

/// Where the node's own point `(px, py)` lands on the canvas.
fn canvas(n: &NodeState, px: f64, py: f64) -> (f64, f64) {
    let p = n.transform * kurbo::Point::new(px, py);
    ((p.x * 1e6).round() / 1e6, (p.y * 1e6).round() / 1e6)
}

#[test]
fn the_anchor_sits_at_the_position_and_the_layer_turns_about_it() {
    let g = graph(
        r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"100x50","at":"500 300","anchor":[0,0],"rotate":90,"scale":[2,1]}]}"#,
    );
    let n = &evaluate(&g, 0.0).nodes[0];
    // The top-left corner is the anchor; x is doubled, then a clockwise quarter turn.
    assert_eq!(canvas(n, 0.0, 0.0), (500.0, 300.0));
    assert_eq!(canvas(n, 100.0, 0.0), (500.0, 500.0));
    assert_eq!(canvas(n, 0.0, 50.0), (450.0, 300.0));
}

#[test]
fn a_child_moves_turns_and_scales_with_its_parent() {
    let g = graph(
        r#"{"motion":1,"fps":10,"dur":"1s","layers":[
        {"id":"label","rect":"10x10","at":"20 0","parent":"card"},
        {"id":"card","rect":"100x100","at":"500 500","anchor":[0,0],"scale":2,"opacity":0.5,
         "anim":{"rotate":"0 -> 90 @0-10f"}}]}"#,
    );
    let s = evaluate(&g, 10.0);
    let label = &s.nodes[0];
    // The label's centre (5, 5 of its box) is at (20, 0) in the card, scaled 2x and turned 90°.
    assert_eq!(canvas(label, 5.0, 5.0), (500.0, 540.0));
    // Opacity is not inherited.
    assert_eq!(opacity(label), 1.0);
    assert_eq!(
        canvas(label, 5.0, 5.0),
        canvas(&evaluate(&g, 10.0).nodes[0], 5.0, 5.0)
    );
}

#[test]
fn a_layer_shows_only_in_its_span() {
    let g = graph(
        r#"{"motion":1,"fps":10,"dur":"2s","layers":[{"id":"a","rect":"10x10","span":"5-15f"}]}"#,
    );
    let shown = |f| evaluate(&g, f).nodes[0].shown;
    assert_eq!(
        (shown(4.9), shown(5.0), shown(14.9), shown(15.0)),
        (false, true, true, false)
    );
}

#[test]
fn keys_run_on_the_layers_own_time() {
    let g = graph(
        r#"{"motion":1,"fps":10,"dur":"5s","layers":[{"id":"a","rect":"10x10","offset":"1s","stretch":2,
        "anim":{"opacity":"0 -> 1 @0-10f"}}]}"#,
    );
    let at = |f| opacity(&evaluate(&g, f).nodes[0]);
    // Its frame 0 is the scene's frame 10, and it runs at half speed.
    assert_eq!(at(10.0), 0.0);
    assert_eq!(at(20.0), 0.5);
    assert_eq!(at(30.0), 1.0);
}
