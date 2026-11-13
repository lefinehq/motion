//! Unit tests for `src/evaluate.rs`.

use super::*;

fn graph(json: &str) -> SceneGraph {
    motion_scene::compile(
        &motion_format::parse_document(json).unwrap().scene,
        &motion_scene::ApproxTextMeasure,
    )
}

/// A 100x100 rect centred at y=700 with a linear 12-frame entrance from frame 15.
const LIN: &str = r#"{"motion":1,"dur":"2s","layers":[{"id":"c","rect":"100x100","at":"center 700","in":"fade-up 60px 12f linear @15f"}]}"#;

#[test]
fn hidden_and_offset_before_the_entrance() {
    let n = &evaluate(&graph(LIN), 0.0).nodes[0];
    assert_eq!((n.opacity, n.y), (0.0, 760.0));
}

#[test]
fn interpolates_during_the_entrance() {
    let n = &evaluate(&graph(LIN), 21.0).nodes[0];
    assert_eq!((n.opacity, n.y), (0.5, 730.0));
    assert_eq!(
        (n.top(), n.bottom(), n.left(), n.right()),
        (680.0, 780.0, 910.0, 1010.0)
    );
}

#[test]
fn settles_at_the_end_and_stays_there() {
    for f in [27.0, 59.0, 1000.0] {
        let n = &evaluate(&graph(LIN), f).nodes[0];
        assert_eq!((n.opacity, n.y), (1.0, 700.0));
    }
}

#[test]
fn layers_without_entrance_are_always_visible() {
    let g = graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"}]}"#);
    assert_eq!(evaluate(&g, 0.0).nodes[0].opacity, 1.0);
}

#[test]
fn zero_duration_jumps_at_its_start_frame() {
    let g =
        graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10","in":"fade 0f @5f"}]}"#);
    assert_eq!(evaluate(&g, 4.9).nodes[0].opacity, 0.0);
    assert_eq!(evaluate(&g, 5.0).nodes[0].opacity, 1.0);
}

#[test]
fn sub_frame_times_are_supported() {
    assert!((evaluate(&graph(LIN), 15.6).nodes[0].opacity - 0.05).abs() < 1e-9);
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
        .find(|n| n.y < 700.0)
        .expect("the curve overshoots above its resting place");
    assert_eq!(past.opacity, 1.0);
    assert_eq!(evaluate(&g, 12.0).nodes[0].y, 700.0);
}
