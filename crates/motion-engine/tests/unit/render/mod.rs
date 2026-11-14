//! Unit tests for `src/render/mod.rs`, and the helpers the other render
//! tests share.

use motion_raster::kurbo::{BezPath, Rect};
use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::Session;

/// The session of the scene JSON `scene`.
pub(crate) fn session(scene: &str) -> Session {
    scene.parse().expect("a valid scene")
}

/// The display list of `scene` at `frame`.
pub(crate) fn list(scene: &str, frame: f64) -> Frame {
    session(scene).display_list(frame).expect("a display list")
}

/// A 200x100 scene at 10 fps lasting 1 s, with `layers`.
pub(crate) fn scene(layers: &str) -> String {
    format!(r#"{{"motion":1,"size":"200x100","fps":10,"dur":"1s","layers":[{layers}]}}"#)
}

/// The one layer of a list that holds one.
pub(crate) fn only_layer(frame: &Frame) -> &Layer {
    match frame.nodes.as_slice() {
        [Node::Layer(layer)] => layer,
        other => panic!("not one layer: {other:?}"),
    }
}

fn path_of(node: &Node) -> &PathDraw {
    match node {
        Node::Path(path) => path,
        other => panic!("not a path: {other:?}"),
    }
}

#[test]
fn the_scene_background_is_drawn_and_blending_is_gamma() {
    let frame = list(
        r##"{"motion":1,"dur":"1s","bg":"#FF000080","layers":[]}"##,
        0.0,
    );
    assert_eq!(
        frame.background,
        Some(Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0))
    );
    assert_eq!(frame.blending, Blending::Gamma);
    assert!(frame.nodes.is_empty());
}

#[test]
fn a_clear_background_is_left_out() {
    let frame = list(
        r##"{"motion":1,"dur":"1s","bg":"#00000000","layers":[]}"##,
        0.0,
    );
    assert_eq!(frame.background, None);
}

#[test]
fn a_rect_layer_is_its_box_filled_under_its_transform() {
    let session = session(&scene(
        r##"{"id":"a","rect":"40x20 r4","fill":"#00FF00","at":"100 50","rotate":30,"opacity":0.5}"##,
    ));
    let state = session.frame(0.0).unwrap();
    let frame = session.display_list(0.0).unwrap();
    let layer = only_layer(&frame);
    assert_eq!(layer.transform, state.nodes[0].transform);
    assert_eq!(layer.opacity, 0.5);
    assert!(layer.effects.is_empty() && layer.clip.is_none() && !layer.isolate);
    let [leaf] = layer.children.as_slice() else {
        panic!("one leaf")
    };
    let rect = path_of(leaf);
    assert_eq!(rect.paint, Paint::Solid(Color::rgba(0.0, 1.0, 0.0, 1.0)));
    assert_eq!(rect.style, DrawStyle::Fill(FillRule::NonZero));
    // Rounded corners leave float dust on the edges.
    assert_eq!(
        rect.path.bounding_box().round(),
        Rect::new(0.0, 0.0, 40.0, 20.0)
    );
}

#[test]
fn rounding_stops_at_half_the_shorter_side() {
    let pill = rect(
        &RectShape {
            width: 40.0,
            height: 20.0,
            radius: 500.0,
        },
        [1.0; 4],
    );
    let capped = rect(
        &RectShape {
            width: 40.0,
            height: 20.0,
            radius: 10.0,
        },
        [1.0; 4],
    );
    assert_eq!(pill, capped);
}

#[rstest]
#[case::hidden(r#""hidden":true"#)]
#[case::outside_its_span(r#""span":"5f-8f""#)]
#[case::transparent(r#""opacity":0"#)]
fn a_layer_that_shows_nothing_is_left_out(#[case] key: &str) {
    let frame = list(
        &scene(&format!(r#"{{"id":"a","rect":"10x10",{key}}}"#)),
        0.0,
    );
    assert!(frame.nodes.is_empty());
}

#[test]
fn a_pivot_draws_nothing_and_its_children_follow_it() {
    let session = session(&scene(
        r#"{"id":"p","pivot":true,"at":"50 50","rotate":90},{"id":"k","rect":"10x10","parent":"p","at":"20 0"}"#,
    ));
    let state = session.frame(0.0).unwrap();
    let frame = session.display_list(0.0).unwrap();
    assert_eq!(only_layer(&frame).transform, state.nodes[1].transform);
}

const COMP: &str = r##"{"motion":1,"size":"200x100","fps":10,"dur":"2s",
  "comps":{"card":{"size":"40x30","fps":10,"dur":"1s","bg":"#102030","layers":[
    {"id":"dot","rect":"10x10","anim":{"opacity":"0 -> 1 @0-10f"}}]}},
  "layers":[{"id":"c","comp":"card","offset":"2f","fx":[{"fill":"#FF0000"}]}]}"##;

#[test]
fn a_comp_is_an_isolated_layer_clipped_to_its_canvas_over_its_background() {
    let session = session(COMP);
    let state = session.frame(7.0).unwrap();
    let frame = session.display_list(7.0).unwrap();
    let layer = only_layer(&frame);
    assert_eq!(layer.transform, state.nodes[0].transform);
    assert_eq!(
        layer.clip,
        Some(Clip::Rect(Rect::new(0.0, 0.0, 40.0, 30.0)))
    );
    assert!(layer.isolate);
    assert_eq!(layer.effects.len(), 1);
    let [background, Node::Layer(dot)] = layer.children.as_slice() else {
        panic!("a background and a layer: {:?}", layer.children)
    };
    let background = path_of(background);
    assert_eq!(
        background.path.bounding_box(),
        Rect::new(0.0, 0.0, 40.0, 30.0)
    );
    assert_eq!(
        background.paint,
        Paint::Solid(color([16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 1.0]))
    );
    // The card's own clock: five frames in, its dot is half faded in.
    assert_eq!(dot.opacity, 0.5);
}

#[test]
fn a_clear_comp_background_draws_nothing() {
    let frame = list(&COMP.replace("#102030", "#10203000"), 7.0);
    assert!(matches!(
        only_layer(&frame).children.as_slice(),
        [Node::Layer(_)]
    ));
}

#[rstest]
#[case::before_its_offset(1.0)]
#[case::past_its_length(2.0 + 10.0)]
fn a_comp_out_of_its_time_draws_nothing(#[case] frame: f64) {
    assert!(list(COMP, frame).nodes.is_empty());
}

#[test]
fn moving_by_the_identity_adds_no_layer() {
    let leaf = fill(BezPath::new(), Color::BLACK);
    assert_eq!(
        moved(Affine::IDENTITY, vec![leaf.clone()]),
        vec![leaf.clone()]
    );
    let shifted = Affine::translate((3.0, 4.0));
    assert_eq!(
        moved(shifted, vec![leaf.clone()]),
        vec![Node::Layer(Layer {
            transform: shifted,
            ..Layer::new(vec![leaf])
        })]
    );
}
