//! Unit tests for `src/pin.rs`.

use super::*;
use rstest::rstest;

fn graph(json: &str) -> SceneGraph {
    crate::compile(
        nakkash_format::parse_document(json).unwrap().scene(),
        &mut nakkash_text::Typesetter::new(nakkash_text::FontSet::builtin()),
    )
    .unwrap()
}

fn points(q: [[f64; 2]; 4]) -> [Point; 4] {
    q.map(|[x, y]| Point::new(x, y))
}

fn close(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-9
}

#[rstest]
#[case::square([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]])]
#[case::perspective([[2.0, 1.0], [9.0, 0.0], [11.0, 12.0], [0.0, 9.0]])]
#[case::mirrored([[10.0, 0.0], [0.0, 0.0], [0.0, 10.0], [10.0, 10.0]])]
fn a_convex_quad_turning_either_way_is_a_pin(#[case] q: [[f64; 2]; 4]) {
    assert_eq!(quad_fault(points(q)), None);
}

#[test]
fn a_quad_whose_bottom_corners_swap_is_crossed_on_its_sides() {
    let q = points([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]]);
    assert_eq!(
        quad_fault(q),
        Some(Fault::Crossed([
            [Corner::TopRight, Corner::BottomRight],
            [Corner::BottomLeft, Corner::TopLeft]
        ]))
    );
    assert_eq!(
        quad_fault(q).unwrap().to_string(),
        "crossed: top-right–bottom-right meets bottom-left–top-left"
    );
}

#[test]
fn a_quad_whose_right_corners_swap_is_crossed_on_its_top_and_bottom() {
    let q = points([[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]]);
    assert_eq!(
        quad_fault(q),
        Some(Fault::Crossed([
            [Corner::TopLeft, Corner::TopRight],
            [Corner::BottomRight, Corner::BottomLeft]
        ]))
    );
}

#[test]
fn a_corner_inside_the_others_bends_the_quad() {
    let q = points([[0.0, 0.0], [10.0, 0.0], [3.0, 3.0], [0.0, 10.0]]);
    assert_eq!(quad_fault(q), Some(Fault::Bent(Corner::BottomRight)));
}

#[rstest]
#[case::in_line([[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [0.0, 10.0]], Corner::TopRight)]
#[case::on_a_neighbour([[0.0, 0.0], [0.0, 0.0], [10.0, 10.0], [0.0, 10.0]], Corner::TopLeft)]
#[case::all_on_a_line([[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]], Corner::TopLeft)]
fn corners_in_line_or_on_one_point_are_flat(#[case] q: [[f64; 2]; 4], #[case] at: Corner) {
    assert_eq!(quad_fault(points(q)), Some(Fault::Flat(at)));
}

/// A frame of a scene whose `ui` layer is pinned.
fn pins_at(json: &str, frame: f64) -> Vec<Pin> {
    let graph = graph(json);
    let state = crate::evaluate(&graph, frame);
    let ui = state.nodes.iter().find(|n| n.id == "ui").unwrap();
    ui.pins.clone()
}

#[test]
fn corners_rest_on_the_box_and_move_by_their_values() {
    let pins = pins_at(
        r#"{"nakkash":1,"size":"400x300","dur":"1s","layers":[
            {"id":"ui","rect":"200x100","fx":[{"corner-pin":[10,5],"bottom-left":[-4,2]}]}]}"#,
        0.0,
    );
    let [pin] = pins.as_slice() else {
        panic!("one pin: {pins:?}")
    };
    assert_eq!(pin.fx, 0);
    assert_eq!(pin.from, Rect::new(0.0, 0.0, 200.0, 100.0));
    assert_eq!(
        pin.to,
        points([[10.0, 5.0], [200.0, 0.0], [200.0, 100.0], [-4.0, 102.0]])
    );
    assert_eq!(pin.fault, None);
}

/// A scene whose four corner pivots sit on a tilted quad, as a track
/// writes them, parented to a `screen` layer moved by `(100, 50)`.
const TRACKED: &str = r#"{"nakkash":1,"size":"800x600","dur":"1s","layers":[
    {"id":"footage","rect":"800x600","position":[100,50],"anchor":[0,0]},
    {"id":"screen-tl","pivot":true,"parent":"footage","position":[200,100]},
    {"id":"screen-tr","pivot":true,"parent":"footage","position":[420,120]},
    {"id":"screen-br","pivot":true,"parent":"footage","position":[400,380]},
    {"id":"screen-bl","pivot":true,"parent":"footage","position":[190,350]},
    {"id":"ui","rect":"120x240","at":"60 120","scale":0.5,
     "fx":[{"corner-pin":[0,0],"pivots":"screen","bottom-right":[3,-2]}]}]}"#;

#[test]
fn corners_rest_where_the_pivots_land_seen_from_the_pinned_layer() {
    let graph = graph(TRACKED);
    let state = crate::evaluate(&graph, 0.0);
    let ui = state.nodes.iter().find(|n| n.id == "ui").unwrap();
    let [pin] = ui.pins.as_slice() else {
        panic!("one pin")
    };
    assert_eq!(pin.fault, None);
    // Through the layer's transform, each corner lands on its pivot (plus
    // the bottom-right offset, in the layer's own px).
    let landed = [
        [300.0, 150.0],
        [520.0, 170.0],
        [500.0, 430.0],
        [290.0, 400.0],
    ];
    for (k, [x, y]) in landed.into_iter().enumerate() {
        let offset = match k {
            2 => ui.transform * (pin.to[k] - Vec2::new(3.0, -2.0)),
            _ => ui.transform * pin.to[k],
        };
        assert!(close(offset, Point::new(x, y)), "corner {k}: {offset:?}");
    }
}

#[test]
fn four_named_layers_pin_in_the_order_written() {
    let json = TRACKED.replace(
        r#""pivots":"screen""#,
        r#""pivots":["screen-tr","screen-br","screen-bl","screen-tl"]"#,
    );
    let pins = pins_at(&json, 0.0);
    let graph = graph(&json);
    let state = crate::evaluate(&graph, 0.0);
    let ui = state.nodes.iter().find(|n| n.id == "ui").unwrap();
    assert!(close(
        ui.transform * pins[0].to[0],
        Point::new(520.0, 170.0)
    ));
}

#[test]
fn pivots_in_the_wrong_order_cross_and_say_so() {
    let json = TRACKED.replace(
        r#""pivots":"screen""#,
        r#""pivots":["screen-tl","screen-tr","screen-bl","screen-br"]"#,
    );
    let pins = pins_at(&json, 0.0);
    assert!(matches!(pins[0].fault, Some(Fault::Crossed(_))), "{pins:?}");
}

#[test]
fn a_pinned_layer_scaled_to_nothing_is_unplaced() {
    let json = TRACKED.replace(r#""scale":0.5"#, r#""scale":0"#);
    let pins = pins_at(&json, 0.0);
    assert_eq!(pins[0].fault, Some(Fault::Unplaced(Corner::TopLeft)));
}

#[test]
fn a_hidden_pin_is_not_resolved_and_the_others_keep_their_place() {
    let pins = pins_at(
        r##"{"nakkash":1,"size":"400x300","dur":"1s","layers":[
            {"id":"ui","rect":"200x100","fx":[
              {"corner-pin":[1,0],"hidden":true},{"fill":"#FF0000"},{"corner-pin":[2,0]}]}]}"##,
        0.0,
    );
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].fx, 1);
    assert_eq!(pins[0].to[0], Point::new(2.0, 0.0));
}

#[test]
fn a_text_layer_pins_a_box_the_size_of_its_composition() {
    let pins = pins_at(
        r#"{"nakkash":1,"size":"400x300","dur":"1s","layers":[
            {"id":"ui","text":"Hi","fx":[{"corner-pin":[0,0]}]}]}"#,
        0.0,
    );
    assert_eq!(pins[0].from, Rect::new(-200.0, -150.0, 200.0, 150.0));
}
