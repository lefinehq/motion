//! Unit tests for `src/evaluate.rs`.

use super::*;
use motion_format::property::Prop;

fn graph(json: &str) -> SceneGraph {
    motion_scene::compile(
        motion_format::parse_document(json).unwrap().scene(),
        &motion_scene::ApproxTextMeasure,
    )
}

/// A 100x100 rect centred at y=700 with a linear 12-frame entrance from frame 15.
const LIN: &str = r#"{"motion":1,"dur":"2s","layers":[{"id":"c","rect":"100x100","at":"center 700","in":"fade-up 60px 12f linear @15f"}]}"#;

#[test]
fn hidden_and_offset_before_the_entrance() {
    let n = &evaluate(&graph(LIN), 0.0).nodes[0];
    assert_eq!((n.get(Prop::OPACITY), n.get(Prop::Y)), (0.0, 760.0));
}

#[test]
fn interpolates_during_the_entrance() {
    let n = &evaluate(&graph(LIN), 21.0).nodes[0];
    assert_eq!((n.get(Prop::OPACITY), n.get(Prop::Y)), (0.5, 730.0));
    assert_eq!(
        (n.top(), n.bottom(), n.left(), n.right()),
        (680.0, 780.0, 910.0, 1010.0)
    );
}

#[test]
fn settles_at_the_end_and_stays_there() {
    for f in [27.0, 59.0, 1000.0] {
        let n = &evaluate(&graph(LIN), f).nodes[0];
        assert_eq!((n.get(Prop::OPACITY), n.get(Prop::Y)), (1.0, 700.0));
    }
}

#[test]
fn layers_without_entrance_are_always_visible() {
    let g = graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"}]}"#);
    assert_eq!(evaluate(&g, 0.0).nodes[0].get(Prop::OPACITY), 1.0);
}

#[test]
fn zero_duration_jumps_at_its_start_frame() {
    let g =
        graph(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10","in":"fade 0f @5f"}]}"#);
    assert_eq!(evaluate(&g, 4.9).nodes[0].get(Prop::OPACITY), 0.0);
    assert_eq!(evaluate(&g, 5.0).nodes[0].get(Prop::OPACITY), 1.0);
}

#[test]
fn sub_frame_times_are_supported() {
    assert!((evaluate(&graph(LIN), 15.6).nodes[0].get(Prop::OPACITY) - 0.05).abs() < 1e-9);
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
        .find(|n| n.get(Prop::Y) < 700.0)
        .expect("the curve overshoots above its resting place");
    assert_eq!(past.get(Prop::OPACITY), 1.0);
    assert_eq!(evaluate(&g, 12.0).nodes[0].get(Prop::Y), 700.0);
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
    assert!((n.get(Prop::OPACITY) - 0.5).abs() < 1e-9);
    assert!((n.get(Prop::SCALE) - 0.8).abs() < 1e-9);
    // Position and rotation are untouched.
    assert_eq!(
        (n.get(Prop::X), n.get(Prop::Y), n.get(Prop::ROTATE)),
        (500.0, 400.0, 0.0)
    );
    let done = &evaluate(&g, 10.0).nodes[0];
    assert_eq!((done.get(Prop::OPACITY), done.get(Prop::SCALE)), (1.0, 1.0));
}

#[test]
fn spin_in_rotates_while_fading_in() {
    let g = preset("spin-in");
    let n = &evaluate(&g, 5.0).nodes[0];
    assert!((n.get(Prop::OPACITY) - 0.5).abs() < 1e-9);
    assert!((n.get(Prop::ROTATE) + 45.0).abs() < 1e-9);
    assert_eq!(evaluate(&g, 10.0).nodes[0].get(Prop::ROTATE), 0.0);
}

#[test]
fn opacity_is_clamped_while_y_overshoots_on_the_same_curve() {
    let g = graph(
        r#"{"motion":1,"dur":"2s","layers":[{"id":"c","rect":"100x100","at":"center 700","in":"fade-up 60px 12f cubic(.3,0,.3,1.6) @0f"}]}"#,
    );
    let mut y_past = false;
    for f in 0..=12 {
        let n = &evaluate(&g, f as f64).nodes[0];
        assert!((0.0..=1.0).contains(&n.get(Prop::OPACITY)), "frame {f}");
        y_past |= n.get(Prop::Y) < 700.0;
    }
    assert!(y_past, "y springs past its resting place");
}

#[test]
fn values_are_parallel_to_the_registry() {
    let n = &evaluate(&graph(LIN), 0.0).nodes[0];
    assert_eq!(n.values.len(), motion_format::property::PROPERTIES.len());
}
