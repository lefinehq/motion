//! Unit tests for `src/round.rs`: which corners are rounded, how far they are
//! cut back and the shape of the curve that replaces them.

use super::*;
use crate::{ContentState, ShapeDraw, compile, evaluate};
use kurbo::Shape;
use nakkash_format::parse_document;
use nakkash_text::{FontSet, Typesetter};

/// The frame-0 draws of a scene whose one layer holds `shapes`.
fn draws(shapes: &str) -> Vec<ShapeDraw> {
    let json = format!(
        r#"{{"nakkash":1,"dur":"1s","fps":10,"layers":[{{"id":"a","shapes":[{shapes}]}}]}}"#
    );
    let doc = parse_document(&json).unwrap();
    let graph = compile(doc.scene(), &mut Typesetter::new(FontSet::builtin())).unwrap();
    match evaluate(&graph, 0.0).nodes.swap_remove(0).content {
        ContentState::Shapes(draws) => draws,
        other => panic!("not shapes: {other:?}"),
    }
}

fn one(shapes: &str) -> BezPath {
    let mut all = draws(shapes);
    assert_eq!(all.len(), 1, "{all:?}");
    all.remove(0).outline
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} vs {b}");
}

const FILL: &str = r##"{"fill":"#FFFFFF"}"##;

fn curves(path: &BezPath) -> usize {
    path.elements()
        .iter()
        .filter(|e| matches!(e, PathEl::CurveTo(..)))
        .count()
}

#[test]
fn a_square_rounded_by_round_corners_is_the_rounded_rectangle() {
    let rounded = one(&format!(
        r#"{{"rect":[100,100]}},{{"round-corners":20}},{FILL}"#
    ));
    let native = one(&format!(r#"{{"rect":[100,100],"round":20}},{FILL}"#));
    assert_eq!(curves(&rounded), 4);
    // The two quarter circles differ only by 0.5519 against 0.55228 of the
    // radius in their handles.
    assert!(
        (rounded.area() - native.area()).abs() < 0.5,
        "{} vs {}",
        rounded.area(),
        native.area()
    );
    let bounds = rounded.bounding_box();
    near(bounds.width(), 100.0);
    near(bounds.height(), 100.0);
}

#[test]
fn a_corner_is_cut_back_by_at_most_half_of_each_side() {
    // Sides of 40 and a radius of 100: each corner is cut 20 back, so the
    // cuts of neighbouring corners meet in the middle of each side.
    let path = round(&kurbo::Rect::new(0.0, 0.0, 40.0, 40.0).to_path(0.1), 100.0);
    let points: Vec<Point> = path
        .elements()
        .iter()
        .filter_map(|e| e.end_point())
        .collect();
    for p in [
        Point::new(20.0, 0.0),
        Point::new(40.0, 20.0),
        Point::new(0.0, 20.0),
    ] {
        assert!(points.contains(&p), "{p:?} in {points:?}");
    }
}

#[test]
fn the_handles_reach_towards_the_corner() {
    let mut l = BezPath::new();
    l.move_to((0.0, 100.0));
    l.line_to((0.0, 0.0));
    l.line_to((100.0, 0.0));
    let path = round(&l, 10.0);
    let els = path.elements();
    // The open ends stay; the corner becomes a line to (0, 10), a curve to
    // (10, 0) whose handles lie 0.5519 of the cut towards the corner, and
    // a line on.
    assert_eq!(els[0], PathEl::MoveTo(Point::new(0.0, 100.0)));
    assert_eq!(els[1], PathEl::LineTo(Point::new(0.0, 10.0)));
    let PathEl::CurveTo(c1, c2, end) = els[2] else {
        panic!("{els:?}")
    };
    near(c1.y, 10.0 * (1.0 - REACH));
    near(c2.x, 10.0 * (1.0 - REACH));
    assert_eq!((c1.x, c2.y, end), (0.0, 0.0, Point::new(10.0, 0.0)));
    assert_eq!(els[3], PathEl::LineTo(Point::new(100.0, 0.0)));
}

#[test]
fn smooth_vertices_and_open_ends_stay_as_they_are() {
    let circle = one(&format!(r#"{{"ellipse":[100,100]}},{FILL}"#));
    let rounded = one(&format!(
        r#"{{"ellipse":[100,100]}},{{"round-corners":30}},{FILL}"#
    ));
    assert_eq!(rounded, circle);
    let line =
        one(r##"{"path":"M0,0 L100,0"},{"round-corners":30},{"stroke":"#FFFFFF","width":2}"##);
    assert_eq!(line.elements().len(), 2);
}

#[test]
fn a_free_path_keeps_its_closing_vertex_once() {
    // A path's segments are cubics even where straight; its closing segment
    // ends on its first vertex, which is rounded once.
    let path = one(&format!(
        r#"{{"path":"M0,0 L100,0 L100,100 Z"}},{{"round-corners":10}},{FILL}"#
    ));
    assert_eq!(curves(&path), 3);
    assert!(path.area().abs() < 5_000.0);
}

#[test]
fn a_radius_of_zero_changes_nothing() {
    let sharp = one(&format!(r#"{{"rect":[100,100]}},{FILL}"#));
    let same = one(&format!(
        r#"{{"rect":[100,100]}},{{"round-corners":0}},{FILL}"#
    ));
    assert_eq!(same, sharp);
}
