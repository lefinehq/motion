//! Unit tests for `src/walk.rs`.

use super::*;
use crate::{SceneGraph, evaluate};
use kurbo::Point;

fn graph(json: &str) -> SceneGraph {
    crate::compile(
        motion_format::parse_document(json).unwrap().scene(),
        &mut motion_text::Typesetter::new(motion_text::FontSet::builtin()),
    )
    .unwrap()
}

/// A 1000x1000 scene: `back`, then `card` (a 400x200 composition placed
/// with its top-left corner at 100,100) holding `panel`, which holds
/// `title` at 10,20 inside it.
const NESTED: &str = r#"{"motion":1,"size":"1000x1000","dur":"1s",
  "comps":{
    "panel":{"size":"200x100","dur":"1s","layers":[{"id":"title","rect":"10x10","position":[15,25]}]},
    "card":{"size":"400x200","dur":"1s","layers":[{"id":"panel","comp":"panel","position":[100,50]}]}},
  "layers":[{"id":"back","rect":"10x10"},{"id":"card","comp":"card","position":[300,200]}]}"#;

fn addresses(state: &FrameState) -> Vec<String> {
    walk(state).map(|v| v.address().to_string()).collect()
}

#[test]
fn every_node_comes_in_paint_order_with_its_address() {
    let state = evaluate(&graph(NESTED), 0.0);
    assert_eq!(
        addresses(&state),
        ["back", "card", "card/panel", "card/panel/title"]
    );
    let depths: Vec<usize> = walk(&state).map(|v| v.depth()).collect();
    assert_eq!(depths, [0, 0, 1, 2]);
}

#[test]
fn a_node_inside_comps_is_placed_in_scene_space() {
    let state = evaluate(&graph(NESTED), 0.0);
    let title = walk(&state).find(|v| v.ids == ["card", "panel", "title"]);
    let title = title.expect("the inner title");
    // The title's own top-left corner, through panel and card, lands where
    // the three offsets add up.
    let corner = title.transform * Point::ZERO;
    let alone = title.node.transform * Point::ZERO;
    let panel = walk(&state).find(|v| v.ids == ["card", "panel"]).unwrap();
    assert_eq!(corner, panel.transform * alone);
    assert_ne!(corner, alone);
}

#[test]
fn a_hidden_comp_node_hides_what_it_holds() {
    // A hidden layer another uses as its matte is still evaluated, so its
    // inside is walked, and drawn by none of it.
    let matte = NESTED
        .replace(r#""comp":"card","#, r#""comp":"card","hidden":true,"#)
        .replace(
            r#"{"id":"back","rect":"10x10"}"#,
            r#"{"id":"back","rect":"10x10","matte":"card"}"#,
        );
    let state = evaluate(&graph(&matte), 0.0);
    let visible: Vec<(String, bool)> = walk(&state)
        .map(|v| (v.address().to_string(), v.visible))
        .collect();
    assert_eq!(
        visible,
        [
            ("back".into(), true),
            ("card".into(), false),
            ("card/panel".into(), false),
            ("card/panel/title".into(), false),
        ]
    );
    // Without the matte, nothing inside it is evaluated at all.
    let hidden = NESTED.replace(r#""comp":"card","#, r#""comp":"card","hidden":true,"#);
    assert_eq!(addresses(&evaluate(&graph(&hidden), 0.0)), ["back", "card"]);
}

#[test]
fn a_comp_node_outside_its_span_has_nothing_inside() {
    let late = NESTED.replace(r#""comp":"card","#, r#""comp":"card","span":"10-20f","#);
    let state = evaluate(&graph(&late), 0.0);
    assert_eq!(addresses(&state), ["back", "card"]);
}

#[test]
fn an_empty_frame_walks_nothing() {
    let state = evaluate(&graph(r#"{"motion":1,"dur":"1s","layers":[]}"#), 0.0);
    assert_eq!(walk(&state).count(), 0);
}
