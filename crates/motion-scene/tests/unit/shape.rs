//! Unit tests for `src/shape.rs`: outlines, which paint covers which
//! outline, draw order and bounds.

use super::*;
use crate::{ContentState, FrameState, compile, evaluate};
use kurbo::{ParamCurveArclen, PathEl};
use motion_format::item::SHAPES;
use motion_format::parse_document;
use motion_format::property::LAYER;
use motion_text::{FontSet, Typesetter};

/// The frame-0 state of a scene whose one layer `a` holds `shapes`.
fn frame(shapes: &str, frame: f64) -> FrameState {
    let json = format!(
        r#"{{"motion":1,"dur":"1s","fps":10,"layers":[{{"id":"a","shapes":[{shapes}]}}]}}"#
    );
    let doc = parse_document(&json).unwrap();
    let graph = compile(doc.scene(), &mut Typesetter::new(FontSet::builtin())).unwrap();
    evaluate(&graph, frame)
}

fn draws_of(shapes: &str) -> Vec<ShapeDraw> {
    match &frame(shapes, 0.0).nodes[0].content {
        ContentState::Shapes(draws) => draws.clone(),
        other => panic!("not shapes: {other:?}"),
    }
}

fn close(a: Rect, b: Rect) {
    let d = [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1];
    assert!(d.iter().all(|v| v.abs() < 1e-6), "{a:?} vs {b:?}");
}

/// The paint colour's red channel, to tell draws apart.
fn red(draw: &ShapeDraw) -> f64 {
    draw.paint.values[0].components()[0]
}

#[test]
fn every_geometry_kind_has_an_outline() {
    for kind in SHAPES.iter().filter(|k| k.role == Role::Geometry) {
        assert!(
            OUTLINES.iter().any(|(name, _)| *name == kind.name),
            "{}",
            kind.name
        );
    }
}

#[test]
fn group_rows_are_the_layer_transform_rows() {
    let names = |r: &'static motion_format::property::Registry| r.names().collect::<Vec<_>>();
    assert_eq!(names(&GROUP), names(&LAYER));
}

#[test]
fn a_rect_is_centred_and_starts_at_the_top_of_its_right_edge() {
    let draws = draws_of(r##"{"rect":[100,40],"position":[10,20]},{"fill":"#FFFFFF"}"##);
    close(
        draws[0].outline.bounding_box(),
        Rect::new(-40.0, 0.0, 60.0, 40.0),
    );
    assert_eq!(
        draws[0].outline.elements()[0],
        PathEl::MoveTo(Point::new(60.0, 0.0))
    );
}

#[test]
fn rounding_is_capped_at_half_the_shorter_side() {
    let pill = draws_of(r##"{"rect":[100,40],"round":500},{"fill":"#FFFFFF"}"##);
    let circle_ends = pill[0]
        .outline
        .segments()
        .map(|s| s.arclen(1e-9))
        .sum::<f64>();
    // Two straight sides of 60 and a circle of radius 20.
    let expected = 2.0 * 60.0 + 2.0 * std::f64::consts::PI * 20.0;
    assert!((circle_ends - expected).abs() < 0.05, "{circle_ends}");
}

#[test]
fn an_ellipse_is_four_quarter_arcs_from_the_top() {
    let draws = draws_of(r##"{"ellipse":[200,100]},{"fill":"#FFFFFF"}"##);
    let outline = &draws[0].outline;
    close(
        outline.bounding_box(),
        Rect::new(-100.0, -50.0, 100.0, 50.0),
    );
    let curves = outline
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::CurveTo(..)))
        .count();
    assert_eq!(curves, 4);
    assert_eq!(
        outline.elements()[0],
        PathEl::MoveTo(Point::new(0.0, -50.0))
    );
}

#[test]
fn a_path_follows_its_vertices_and_closes() {
    let draws = draws_of(r##"{"path":"M0,0 L10,0 C10,5 5,10 0,10 Z"},{"fill":"#FFFFFF"}"##);
    let elements = draws[0].outline.elements();
    assert_eq!(elements.first(), Some(&PathEl::MoveTo(Point::ORIGIN)));
    assert_eq!(elements.last(), Some(&PathEl::ClosePath));
    close(
        draws[0].outline.bounding_box(),
        Rect::new(0.0, 0.0, 10.0, 10.0),
    );
}

#[test]
fn a_paint_covers_only_the_outlines_before_it() {
    // The fill between the two rects covers only the first.
    let draws =
        draws_of(r##"{"rect":[10,10]},{"fill":"#FF0000"},{"rect":[50,50]},{"fill":"#000000"}"##);
    assert_eq!(draws.len(), 2);
    // Back to front: the later fill, covering both, is at the back.
    assert_eq!(red(&draws[1]), 1.0);
    close(
        draws[1].outline.bounding_box(),
        Rect::new(-5.0, -5.0, 5.0, 5.0),
    );
    close(
        draws[0].outline.bounding_box(),
        Rect::new(-25.0, -25.0, 25.0, 25.0),
    );
}

#[test]
fn a_paint_with_nothing_before_it_draws_nothing() {
    assert!(draws_of(r##"{"fill":"#FF0000"},{"rect":[10,10]}"##).is_empty());
}

#[test]
fn an_outer_paint_covers_groups_through_their_transforms() {
    let draws = draws_of(
        r##"{"group":[{"rect":[10,10]},{"fill":"#FF0000"}],"position":[100,0],"scale":2,"opacity":0.5},{"stroke":"#000000","width":4}"##,
    );
    assert_eq!(draws.len(), 2);
    let (outer, inner) = (&draws[0], &draws[1]);
    // The inner fill draws in the group's space, at the group's opacity.
    assert_eq!((red(inner), inner.opacity), (1.0, 0.5));
    close(
        inner.outline.bounding_box(),
        Rect::new(-5.0, -5.0, 5.0, 5.0),
    );
    close(inner.bounds(), Rect::new(90.0, -10.0, 110.0, 10.0));
    // The outer stroke sees the rect moved and scaled, at full opacity, and
    // its width stays in its own space.
    assert_eq!(outer.opacity, 1.0);
    close(
        outer.outline.bounding_box(),
        Rect::new(90.0, -10.0, 110.0, 10.0),
    );
    close(outer.bounds(), Rect::new(88.0, -12.0, 112.0, 12.0));
}

#[test]
fn hidden_items_draw_and_cover_nothing() {
    let draws = draws_of(
        r##"{"rect":[10,10],"hidden":true},{"rect":[4,4]},{"fill":"#FF0000"},{"fill":"#000000","hidden":true}"##,
    );
    assert_eq!(draws.len(), 1);
    close(
        draws[0].outline.bounding_box(),
        Rect::new(-2.0, -2.0, 2.0, 2.0),
    );
}

#[test]
fn a_shape_layer_is_as_big_as_its_draws() {
    let state = frame(
        r##"{"rect":[10,10],"position":[-20,0]},{"ellipse":[6,6],"position":[30,0]},{"stroke":"#000000","width":2}"##,
        0.0,
    );
    close(
        state.nodes[0].local_bounds,
        Rect::new(-26.0, -6.0, 34.0, 6.0),
    );
    let empty = frame("", 0.0);
    assert_eq!(empty.nodes[0].local_bounds, Rect::ZERO);
}

#[test]
fn item_tracks_run_on_the_layer_clock() {
    let shapes = r##"{"rect":[10,10],"anim":{"size":[{"t":0,"v":[10,10]},{"t":10,"v":[30,50]}]}},{"fill":"#FFFFFF","anim":{"opacity":"0 -> 1 @0-10f"}}"##;
    let draws = match &frame(shapes, 5.0).nodes[0].content {
        ContentState::Shapes(draws) => draws.clone(),
        _ => unreachable!(),
    };
    close(
        draws[0].outline.bounding_box(),
        Rect::new(-10.0, -15.0, 10.0, 15.0),
    );
    assert_eq!(draws[0].paint.number(props::FILL_OPACITY), 0.5);
}

#[test]
fn a_path_morphs_vertex_by_vertex() {
    let shapes = r##"{"path":"M0,0 L10,0 L10,10 Z","anim":{"path":[{"t":0,"v":"M0,0 L10,0 L10,10 Z"},{"t":10,"v":"M0,0 L30,0 L30,30 Z"}]}},{"fill":"#FFFFFF"}"##;
    let draws = match &frame(shapes, 5.0).nodes[0].content {
        ContentState::Shapes(draws) => draws.clone(),
        _ => unreachable!(),
    };
    close(
        draws[0].outline.bounding_box(),
        Rect::new(0.0, 0.0, 20.0, 20.0),
    );
}

#[test]
fn outline_is_none_for_items_that_are_not_geometry() {
    let draws = draws_of(r##"{"rect":[1,1]},{"fill":"#FFFFFF"}"##);
    assert!(outline(&draws[0].paint).is_none());
    assert_eq!(draws[0].paint.setting("rule"), "nonzero");
    assert_eq!(draws[0].paint.color(props::FILL_COLOR), [1.0; 4]);
}
