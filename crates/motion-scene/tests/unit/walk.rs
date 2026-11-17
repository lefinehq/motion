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

fn addresses(graph: &SceneGraph, state: &FrameState) -> Vec<String> {
    walk(graph, state)
        .map(|v| v.address().to_string())
        .collect()
}

#[test]
fn every_node_comes_in_paint_order_with_its_address() {
    let g = graph(NESTED);
    let state = evaluate(&g, 0.0);
    assert_eq!(
        addresses(&g, &state),
        ["back", "card", "card/panel", "card/panel/title"]
    );
    let depths: Vec<usize> = walk(&g, &state).map(|v| v.depth()).collect();
    assert_eq!(depths, [0, 0, 1, 2]);
}

#[test]
fn a_node_inside_comps_is_placed_in_scene_space() {
    let g = graph(NESTED);
    let state = evaluate(&g, 0.0);
    let title = walk(&g, &state).find(|v| v.ids == ["card", "panel", "title"]);
    let title = title.expect("the inner title");
    // The title's own top-left corner, through panel and card, lands where
    // the three offsets add up.
    let corner = title.transform * Point::ZERO;
    let alone = title.node.transform * Point::ZERO;
    let panel = walk(&g, &state)
        .find(|v| v.ids == ["card", "panel"])
        .unwrap();
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
    let g = graph(&matte);
    let state = evaluate(&g, 0.0);
    let visible: Vec<(String, bool)> = walk(&g, &state)
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
    let g = graph(&hidden);
    assert_eq!(addresses(&g, &evaluate(&g, 0.0)), ["back", "card"]);
}

#[test]
fn a_comp_node_outside_its_span_has_nothing_inside() {
    let late = NESTED.replace(r#""comp":"card","#, r#""comp":"card","span":"10-20f","#);
    let g = graph(&late);
    let state = evaluate(&g, 0.0);
    assert_eq!(addresses(&g, &state), ["back", "card"]);
}

#[test]
fn an_empty_frame_walks_nothing() {
    let g = graph(r#"{"motion":1,"dur":"1s","layers":[]}"#);
    let state = evaluate(&g, 0.0);
    assert_eq!(walk(&g, &state).count(), 0);
}

#[test]
fn paths_list_every_node_as_the_walk_meets_it() {
    let g = graph(NESTED);
    let state = evaluate(&g, 0.0);
    let walked: Vec<Vec<&str>> = walk(&g, &state).map(|v| v.ids).collect();
    assert_eq!(g.paths(), walked);
}

#[test]
fn the_index_is_the_place_in_paths_even_past_a_comp_not_shown() {
    let twice = NESTED.replace(
        r#"{"id":"card","comp":"card","position":[300,200]}"#,
        r#"{"id":"early","comp":"card","span":"10-20f"},{"id":"card","comp":"card","position":[300,200]}"#,
    );
    let g = graph(&twice);
    let paths = g.paths();
    let state = evaluate(&g, 0.0);
    for visit in walk(&g, &state) {
        assert_eq!(paths[visit.index], visit.ids);
    }
    assert_eq!(walk(&g, &state).count(), 5);
    assert_eq!(paths.len(), 7);
}

#[test]
fn opacity_multiplies_down_the_tree() {
    let faded = NESTED
        .replace(r#""comp":"card","#, r#""comp":"card","opacity":0.5,"#)
        .replace(r#""comp":"panel","#, r#""comp":"panel","opacity":0.5,"#);
    let g = graph(&faded);
    let state = evaluate(&g, 0.0);
    let opacity: Vec<f64> = walk(&g, &state).map(|v| v.opacity).collect();
    assert_eq!(opacity, [1.0, 0.5, 0.25, 0.25]);
}

#[test]
fn a_comp_clips_what_it_holds_to_its_canvas() {
    let g = graph(NESTED);
    let state = evaluate(&g, 0.0);
    let visits: Vec<Visit> = walk(&g, &state).collect();
    assert_eq!(visits[0].clip, None);
    assert_eq!(visits[1].clip, None);
    // `panel` sits in card's canvas; `title` in panel's, inside card's.
    let card = visits[1]
        .transform
        .transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, 400.0, 200.0));
    assert_eq!(visits[2].clip, Some(card));
    let panel = visits[2]
        .transform
        .transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, 200.0, 100.0));
    assert_eq!(visits[3].clip, Some(card.intersect(panel)));
    assert_eq!(visits[3].drawn(), Some(visits[3].bounds()));
}

#[test]
fn a_node_clipped_away_draws_nothing() {
    // The title sits past panel's 200x100 canvas.
    let outside = NESTED.replace("[15,25]", "[500,500]");
    let g = graph(&outside);
    let state = evaluate(&g, 0.0);
    let title = walk(&g, &state).last().unwrap();
    assert_eq!(title.ids, ["card", "panel", "title"]);
    assert_eq!(title.drawn(), None);
}

/// A 1000x1000 scene: a 400x100 `slot` centred at 500,900, then a 400x100
/// `title` centred at 500,960 matted by it as `matte` says.
fn matted(matte: &str) -> SceneGraph {
    graph(&format!(
        r#"{{"motion":1,"size":"1000x1000","dur":"1s","layers":[
          {{"id":"slot","rect":"400x100","position":[500,900]}},
          {{"id":"title","rect":"400x100","position":[500,960],"matte":"{matte}"}}]}}"#
    ))
}

#[rstest::rstest]
#[case::alpha("slot", Some(kurbo::Rect::new(300.0, 850.0, 700.0, 950.0)))]
#[case::luma("slot luma", Some(kurbo::Rect::new(300.0, 850.0, 700.0, 950.0)))]
// An inverted matte shows the layer where the slot does not draw.
#[case::alpha_inverted("slot alpha-inverted", None)]
#[case::luma_inverted("slot luma-inverted", None)]
fn a_matte_clips_a_node_to_what_its_matte_layer_draws(
    #[case] matte: &str,
    #[case] clip: Option<kurbo::Rect>,
) {
    let g = matted(matte);
    let state = evaluate(&g, 0.0);
    let title = walk(&g, &state).last().unwrap();
    assert_eq!(title.clip, clip);
}

#[test]
fn a_matte_layer_outside_its_span_lets_nothing_draw() {
    let g = graph(
        r#"{"motion":1,"size":"1000x1000","dur":"1s","layers":[
          {"id":"slot","rect":"400x100","span":"0.5s-1s"},
          {"id":"title","rect":"400x100","matte":"slot"}]}"#,
    );
    let state = evaluate(&g, 0.0);
    assert_eq!(walk(&g, &state).last().unwrap().drawn(), None);
    let state = evaluate(&g, 20.0);
    assert!(walk(&g, &state).last().unwrap().drawn().is_some());
}
