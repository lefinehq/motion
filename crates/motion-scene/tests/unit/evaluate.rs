//! Unit tests for `src/evaluate.rs`.

use super::*;
use motion_format::property::Prop;
use rstest::rstest;

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

/// Hidden and offset before the entrance, interpolated during it, then settled.
#[rstest]
#[case::before_the_entrance(0.0, 0.0, 760.0)]
#[case::during_the_entrance(21.0, 0.5, 730.0)]
#[case::at_the_end(27.0, 1.0, 700.0)]
#[case::after_the_end(59.0, 1.0, 700.0)]
#[case::long_after_the_end(1000.0, 1.0, 700.0)]
fn entrance_opacity_and_y(#[case] frame: f64, #[case] want_opacity: f64, #[case] want_y: f64) {
    let n = &evaluate(&graph(LIN), frame).nodes[0];
    assert_eq!((opacity(n), y(n)), (want_opacity, want_y));
}

#[test]
fn the_box_moves_with_the_entrance() {
    let n = &evaluate(&graph(LIN), 21.0).nodes[0];
    assert_eq!(
        (n.top(), n.bottom(), n.left(), n.right()),
        (680.0, 780.0, 910.0, 1010.0)
    );
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

/// A 25 fps scene drawing composition `card` (10 fps, 2 s, 400x200) from
/// layer `c`, which starts at frame 25 at half speed; inside, `dot` fades in
/// over the card's first second.
const COMP: &str = r##"{"motion":1,"dur":"10s","fps":25,
  "comps":{"card":{"size":"400x200","fps":10,"dur":"2s","bg":"#102030","layers":[
    {"id":"dot","rect":"10x10","anim":{"opacity":"0 -> 1 @0-10f"}}]}},
  "layers":[{"id":"c","comp":"card","offset":"25f","stretch":2,"hidden":false}]}"##;

fn inner(state: &FrameState) -> Option<&FrameState> {
    match &state.nodes[0].content {
        ContentState::Comp(0, inner) => inner.as_deref(),
        other => panic!("not the card: {other:?}"),
    }
}

#[rstest]
// Before its offset the card's time is negative: nothing shows.
#[case::before_the_offset(10.0, None)]
// 25 frames in at half speed is half a second of the card's time: frame 5.
#[case::half_way_through_the_fade(50.0, Some(0.5))]
#[case::faded_in(100.0, Some(1.0))]
// The card lasts 2 s, which at half speed ends 4 s past the offset.
#[case::past_the_card(25.0 + 100.0, None)]
fn a_comp_runs_on_its_layer_clock(#[case] frame: f64, #[case] dot_opacity: Option<f64>) {
    let graph = graph(COMP);
    let state = evaluate(&graph, frame);
    assert_eq!(state.nodes[0].shown, dot_opacity.is_some());
    assert_eq!(inner(&state).map(|s| opacity(&s.nodes[0])), dot_opacity);
}

#[test]
fn a_comp_compiles_its_canvas_and_the_layer_box() {
    let graph = graph(COMP);
    let card = &graph.comps[0];
    assert_eq!(
        (card.id.as_str(), card.width, card.height),
        ("card", 400.0, 200.0)
    );
    assert_eq!((card.fps, card.duration_frames), (10.0, 20.0));
    assert_eq!(card.background[3], 1.0);
    assert_eq!(graph.nodes[0].content, NodeContent::Comp(0));
    // The layer's box is the card, centred on the canvas.
    let state = evaluate(&graph, 50.0);
    assert_eq!(
        (state.nodes[0].width, state.nodes[0].height),
        (400.0, 200.0)
    );
    assert_eq!(state.nodes[0].bounds().x0, (1920.0 - 400.0) / 2.0);
    assert_eq!(evaluate_comp(&graph, 0, 5.0).nodes[0].id, "dot");
}

#[test]
fn a_hidden_comp_is_not_evaluated_inside() {
    let graph = graph(&COMP.replace(r#""hidden":false"#, r#""hidden":true"#));
    let state = evaluate(&graph, 50.0);
    assert!(state.nodes[0].shown && !state.nodes[0].visible());
    assert!(inner(&state).is_none());
}

#[test]
fn effects_are_evaluated_on_the_layer_clock() {
    let graph = graph(
        r##"{"motion":1,"dur":"1s","fps":10,"layers":[{"id":"a","rect":"10x10","offset":"5f",
          "fx":[{"fill":"#FF0000","anim":{"opacity":"0 -> 1 @0-10f"}},{"fill":"#000000","hidden":true}]}]}"##,
    );
    let state = evaluate(&graph, 10.0);
    let fx = &state.nodes[0].fx;
    assert_eq!(fx.len(), 1);
    assert_eq!(
        fx[0].number(motion_format::item::props::FILL_EFFECT_OPACITY),
        0.5
    );
}

#[test]
fn pivots_draw_nothing_and_children_follow_them() {
    let graph = graph(
        r#"{"motion":1,"dur":"1s","layers":[{"id":"p","pivot":true,"at":"100 100","rotate":90},
          {"id":"k","rect":"10x10","parent":"p","at":"20 0"}]}"#,
    );
    let state = evaluate(&graph, 0.0);
    assert_eq!(state.nodes[0].content, ContentState::Pivot);
    assert_eq!(state.nodes[0].local_bounds, Rect::ZERO);
    let child = state.nodes[1].bounds();
    // 20 px along the pivot's x axis, which points down after a quarter turn.
    assert_eq!((child.center().x, child.center().y), (100.0, 120.0));
}
