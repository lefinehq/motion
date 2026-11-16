//! Unit tests for `src/expand.rs`: which way outlines move, how parted
//! sides join, how crossing sides are cut, open outlines and copies.

use super::*;
use crate::{ContentState, ShapeDraw, compile, evaluate};
use kurbo::{Rect, Shape};
use motion_format::parse_document;
use motion_text::{FontSet, Typesetter};

/// The frame-0 draws of a scene whose one layer holds `shapes`.
fn draws(shapes: &str) -> Vec<ShapeDraw> {
    let json = format!(
        r#"{{"motion":1,"dur":"1s","fps":10,"layers":[{{"id":"a","shapes":[{shapes}]}}]}}"#
    );
    let doc = parse_document(&json).unwrap();
    let graph = compile(doc.scene(), &mut Typesetter::new(FontSet::builtin())).unwrap();
    match evaluate(&graph, 0.0).nodes.swap_remove(0).content {
        ContentState::Shapes(draws) => draws,
        other => panic!("not shapes: {other:?}"),
    }
}

/// The one outline a scene's shapes draw.
fn one(shapes: &str) -> BezPath {
    let mut all = draws(shapes);
    assert_eq!(all.len(), 1, "{all:?}");
    all.remove(0).outline
}

const FILL: &str = r##"{"fill":"#FFFFFF"}"##;

fn near(a: f64, b: f64, by: f64) {
    assert!((a - b).abs() <= by, "{a} vs {b}");
}

fn same_box(a: Rect, b: Rect, by: f64) {
    for (x, y) in [(a.x0, b.x0), (a.y0, b.y0), (a.x1, b.x1), (a.y1, b.y1)] {
        near(x, y, by);
    }
}

fn subpaths(path: &BezPath) -> usize {
    path.elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count()
}

#[test]
fn a_rectangle_grows_on_every_side_with_square_corners() {
    let path = one(&format!(r#"{{"rect":[100,50]}},{{"expand":10}},{FILL}"#));
    same_box(
        path.bounding_box(),
        Rect::new(-60.0, -35.0, 60.0, 35.0),
        1e-9,
    );
    near(path.area().abs(), 120.0 * 70.0, 1e-6);
}

#[test]
fn a_negative_amount_shrinks() {
    let path = one(&format!(r#"{{"rect":[100,50]}},{{"expand":-10}},{FILL}"#));
    same_box(
        path.bounding_box(),
        Rect::new(-40.0, -15.0, 40.0, 15.0),
        1e-9,
    );
    near(path.area().abs(), 80.0 * 30.0, 1e-6);
}

#[test]
fn an_outline_running_counterclockwise_moves_the_other_way() {
    let path = one(&format!(
        r#"{{"path":"M0,0 L0,50 L100,50 L100,0 Z"}},{{"expand":10}},{FILL}"#
    ));
    same_box(path.bounding_box(), Rect::new(10.0, 10.0, 90.0, 40.0), 1e-9);
}

#[test]
fn a_rounded_rectangle_grows_its_rounding_with_it() {
    let grown = one(&format!(
        r#"{{"rect":[100,50],"round":10}},{{"expand":10}},{FILL}"#
    ));
    let drawn = one(&format!(r#"{{"rect":[120,70],"round":20}},{FILL}"#));
    // Quarter circles drawn as cubics bulge out by up to 0.03% of their
    // radius, and the moved ones by the same distance, not 0.03% of theirs.
    near(grown.area(), drawn.area(), 0.5);
    same_box(grown.bounding_box(), drawn.bounding_box(), 1e-3);
}

#[test]
fn an_ellipse_grows_into_the_ellipse_around_it() {
    let grown = one(&format!(
        r#"{{"ellipse":[100,100]}},{{"expand":20}},{FILL}"#
    ));
    let drawn = one(&format!(r#"{{"ellipse":[140,140]}},{FILL}"#));
    // Cubic quarter circles bulge as above: 0.03% of 50 px around 440 px.
    near(grown.area(), drawn.area(), 6.0);
    same_box(grown.bounding_box(), drawn.bounding_box(), 1e-2);
}

#[test]
fn a_round_join_rounds_the_corners_by_the_amount() {
    let path = one(&format!(
        r#"{{"rect":[100,50]}},{{"expand":10,"join":"round"}},{FILL}"#
    ));
    // The arcs bulge as cubic quarter circles do.
    near(
        path.area(),
        120.0 * 70.0 - (4.0 - std::f64::consts::PI) * 100.0,
        0.5,
    );
}

#[test]
fn a_bevel_join_cuts_the_corners() {
    let path = one(&format!(
        r#"{{"rect":[100,50]}},{{"expand":10,"join":"bevel"}},{FILL}"#
    ));
    // Each corner loses half of a 10 by 10 square.
    near(path.area(), 120.0 * 70.0 - 4.0 * 50.0, 1e-6);
}

#[test]
fn a_miter_past_its_limit_is_bevelled() {
    // A sharp spike: the miter at its tip reaches far past 2 amounts.
    let spike = r#"{"path":"M0,0 L100,10 L0,20 Z"}"#;
    let mitred = one(&format!(r#"{spike},{{"expand":5,"miter":100}},{FILL}"#));
    let cut = one(&format!(r#"{spike},{{"expand":5,"miter":2}},{FILL}"#));
    assert!(
        mitred.bounding_box().x1 > 150.0,
        "{:?}",
        mitred.bounding_box()
    );
    assert!(cut.bounding_box().x1 < 106.0, "{:?}", cut.bounding_box());
}

#[test]
fn sides_that_cross_in_a_corner_are_cut_where_they_cross() {
    // An L whose inner corner the moved sides cross in.
    let l = r#"{"path":"M0,0 L100,0 L100,40 L40,40 L40,100 L0,100 Z"}"#;
    let path = one(&format!("{l},{{\"expand\":10}},{FILL}"));
    // The grown L: 120 by 120 less the 60 by 60 notch, through a corner
    // with no loop left over.
    near(path.area(), 120.0 * 120.0 - 60.0 * 60.0, 1e-6);
}

#[test]
fn an_open_line_becomes_one_closed_outline_around_it() {
    let path = one(&format!(
        r#"{{"path":"M0,0 L100,0"}},{{"expand":5}},{FILL}"#
    ));
    assert_eq!(subpaths(&path), 1);
    assert!(matches!(path.elements().last(), Some(PathEl::ClosePath)));
    // The ends are reversals, which no miter reaches, so they are bevelled
    // straight across: a 100 by 10 box.
    near(path.area().abs(), 1000.0, 1e-6);
    same_box(path.bounding_box(), Rect::new(0.0, -5.0, 100.0, 5.0), 1e-9);
}

#[test]
fn copies_move_one_amount_further_each() {
    let path = one(&format!(
        r#"{{"rect":[100,100]}},{{"expand":10,"copies":3}},{FILL}"#
    ));
    assert_eq!(subpaths(&path), 3);
    same_box(
        path.bounding_box(),
        Rect::new(-80.0, -80.0, 80.0, 80.0),
        1e-9,
    );
    let shifted = one(&format!(
        r#"{{"rect":[100,100]}},{{"expand":10,"copies":2,"copy-offset":-1}},{FILL}"#
    ));
    // Copies at 0 and 1 amounts: the outline itself and one grown.
    same_box(
        shifted.bounding_box(),
        Rect::new(-60.0, -60.0, 60.0, 60.0),
        1e-9,
    );
    assert_eq!(subpaths(&shifted), 2);
}

#[test]
fn no_copies_leave_nothing() {
    let all = draws(&format!(
        r#"{{"rect":[100,100]}},{{"expand":10,"copies":0}},{FILL}"#
    ));
    assert!(all.is_empty(), "{all:?}");
}

#[test]
fn an_expansion_reaches_into_the_groups_above_it_in_their_space() {
    // The rectangle is in a group scaled by 2; it grows by 10 in its own
    // space, 20 in the layer's.
    let path = draws(&format!(
        r#"{{"group":[{{"rect":[100,50]}},{FILL}],"scale":[2,2]}},{{"expand":10}}"#
    ))
    .remove(0);
    same_box(
        path.transform
            .transform_rect_bbox(path.outline.bounding_box()),
        Rect::new(-120.0, -70.0, 120.0, 70.0),
        1e-9,
    );
}

#[test]
fn a_zero_amount_leaves_the_outline_as_it_is() {
    let rect = r#"{"rect":[100,50],"round":10}"#;
    assert_eq!(
        one(&format!(r#"{rect},{{"expand":0}},{FILL}"#)),
        one(&format!("{rect},{FILL}"))
    );
}
