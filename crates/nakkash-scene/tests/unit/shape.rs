//! Unit tests for `src/shape.rs`: outlines, which paint covers which
//! outline, draw order and bounds.

use super::*;
use crate::{ContentState, FrameState, compile, evaluate};
use kurbo::{ParamCurve, ParamCurveArclen, PathEl};
use nakkash_format::item::SHAPES;
use nakkash_format::parse_document;
use nakkash_format::property::LAYER;
use nakkash_text::{FontSet, Typesetter};

/// The frame-0 state of a scene whose one layer `a` holds `shapes`.
fn frame(shapes: &str, frame: f64) -> FrameState {
    let json = format!(
        r#"{{"nakkash":1,"dur":"1s","fps":10,"layers":[{{"id":"a","shapes":[{shapes}]}}]}}"#
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
fn every_modifier_kind_has_a_row() {
    for kind in SHAPES.iter().filter(|k| k.role == Role::Modifier) {
        assert!(
            MODIFIERS.iter().any(|(name, _)| *name == kind.name),
            "{}",
            kind.name
        );
    }
}

#[test]
fn group_rows_are_the_layer_transform_rows() {
    let names = |r: &'static nakkash_format::property::Registry| r.names().collect::<Vec<_>>();
    // A layer then has the gain of its sound, which a group does not.
    assert_eq!(names(&GROUP), names(&LAYER)[..5]);
    assert_eq!(names(&LAYER)[5], "volume");
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

/// The corners of a star's outline, in order: the end of every segment.
fn corners(star: &str) -> Vec<Point> {
    let draws = draws_of(&format!(r##"{star},{{"fill":"#FFFFFF"}}"##));
    draws[0].outline.segments().map(|s| s.eval(1.0)).collect()
}

fn near(a: Point, b: Point) {
    assert!((a - b).hypot() < 1e-9, "{a:?} vs {b:?}");
}

#[test]
fn a_polygon_starts_straight_up_and_runs_clockwise() {
    let at = corners(r#"{"star":4,"type":"polygon","outer-radius":10,"position":[5,0]}"#);
    let expected = [(15.0, 0.0), (5.0, 10.0), (-5.0, 0.0), (5.0, -10.0)];
    assert_eq!(at.len(), 4);
    for (got, want) in at.iter().zip(expected) {
        near(*got, Point::from(want));
    }
    // Sharp corners join with straight sides.
    let draws = draws_of(r##"{"star":4,"type":"polygon"},{"fill":"#FFFFFF"}"##);
    assert!(
        draws[0]
            .outline
            .elements()
            .iter()
            .all(|e| !matches!(e, PathEl::CurveTo(..)))
    );
}

#[test]
fn a_star_alternates_its_points_with_its_inner_corners() {
    let at = corners(r#"{"star":2,"outer-radius":10,"inner-radius":4}"#);
    let expected = [(4.0, 0.0), (0.0, 10.0), (-4.0, 0.0), (0.0, -10.0)];
    for (got, want) in at.iter().zip(expected) {
        near(*got, Point::from(want));
    }
    // Five points by default, each with an inner corner.
    assert_eq!(corners(r#"{"star":5}"#).len(), 10);
}

#[test]
fn rotate_turns_the_first_point_clockwise() {
    let at = corners(r#"{"star":3,"type":"polygon","outer-radius":10,"rotate":90}"#);
    // The last segment closes back to the first point, now to the right.
    near(at[2], Point::new(10.0, 0.0));
}

#[test]
fn counterclockwise_walks_the_same_corners_the_other_way() {
    let ahead = corners(r#"{"star":5,"rotate":10}"#);
    let back = corners(r#"{"star":5,"rotate":10,"direction":"counterclockwise"}"#);
    // Both start, and close, at the first point; between, one is the other reversed.
    near(ahead[9], back[9]);
    for (a, b) in ahead[..9].iter().zip(back[..9].iter().rev()) {
        near(*a, *b);
    }
}

#[test]
fn points_count_by_their_whole_part() {
    assert_eq!(corners(r#"{"star":4.9,"type":"polygon"}"#).len(), 4);
    let none = draws_of(r##"{"star":0.5},{"fill":"#FFFFFF"}"##);
    assert!(none.is_empty(), "{none:?}");
}

#[test]
fn roundness_sets_handles_along_the_circle_a_quarter_arc_long() {
    let n = 4.0;
    let (r, ri) = (100.0, 40.0);
    let draws = draws_of(
        r##"{"star":4,"outer-radius":100,"inner-radius":40,"outer-roundness":0.5,"inner-roundness":-0.25},{"fill":"#FFFFFF"}"##,
    );
    let PathEl::CurveTo(leaving, arriving, inner) = draws[0].outline.elements()[1] else {
        panic!("a curve: {:?}", draws[0].outline.elements());
    };
    let quarter = |radius: f64| radius * std::f64::consts::TAU / (4.0 * n);
    // The first point is straight up: its handle runs right, along the circle.
    near(leaving, Point::new(0.5 * quarter(r), -r));
    // The first inner corner is 45° on; a negative roundness turns its handle back.
    let angle = (-45.0_f64).to_radians();
    near(inner, Point::new(ri * angle.cos(), ri * angle.sin()));
    let along = Vec2::new(-angle.sin(), angle.cos());
    near(arriving, inner - along * (-0.25 * quarter(ri)));
}

#[test]
fn a_polygon_ignores_the_inner_corners() {
    assert_eq!(
        corners(r#"{"star":6,"type":"polygon","inner-radius":1,"inner-roundness":1}"#),
        corners(r#"{"star":6,"type":"polygon"}"#)
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
