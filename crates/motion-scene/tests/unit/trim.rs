//! Unit tests for `src/trim.rs`: cut lengths and ends, the stretch rules and
//! which outlines a trim reaches.

use super::*;
use crate::{ContentState, ShapeDraw, compile, evaluate};
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

fn length(path: &BezPath) -> f64 {
    path.segments().map(|s| s.arclen(ARC_ACCURACY)).sum()
}

fn pieces(path: &BezPath) -> usize {
    path.elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count()
}

fn closed(path: &BezPath) -> bool {
    path.elements()
        .iter()
        .any(|e| matches!(e, PathEl::ClosePath))
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-4, "{a} vs {b}");
}

const LINE: &str = r#"{"path":"M0,0 L100,0"}"#;
const STROKE: &str = r##"{"stroke":"#FFFFFF","width":2}"##;

fn one(shapes: &str) -> BezPath {
    let mut all = draws(shapes);
    assert_eq!(all.len(), 1, "{all:?}");
    all.remove(0).outline
}

#[test]
fn a_line_cut_in_half_keeps_its_first_half() {
    let path = one(&format!(r#"{LINE},{{"trim":0.5}},{STROKE}"#));
    near(length(&path), 50.0);
    let segs: Vec<_> = path.segments().collect();
    assert_eq!(segs[0].start(), Point::new(0.0, 0.0));
    near(segs[segs.len() - 1].end().x, 50.0);
}

#[test]
fn a_cut_rectangle_is_open_and_as_long_as_its_share() {
    let path = one(&format!(r#"{{"rect":[100,40]}},{{"trim":0.25}},{STROKE}"#));
    near(length(&path), 70.0);
    assert!(!closed(&path));
}

#[test]
fn a_cut_circle_follows_its_arc() {
    let path = one(&format!(
        r#"{{"ellipse":[100,100]}},{{"trim":0.5}},{STROKE}"#
    ));
    let whole = one(&format!(r#"{{"ellipse":[100,100]}},{STROKE}"#));
    near(length(&path), length(&whole) / 2.0);
    // The four quarter cubics follow the arc to within 0.03%.
    assert!((length(&path) / (std::f64::consts::PI * 50.0) - 1.0).abs() < 3e-4);
    let end = path.segments().last().unwrap().end();
    near(end.x, 0.0);
    near(end.y, 50.0);
}

#[test]
fn a_cut_star_follows_its_corners_in_its_direction() {
    let square = r#"{"star":4,"type":"polygon","outer-radius":50}"#;
    let path = one(&format!(r#"{square},{{"trim":0.25}},{STROKE}"#));
    let whole = one(&format!(r#"{square},{STROKE}"#));
    near(length(&whole), 4.0 * 50.0 * std::f64::consts::SQRT_2);
    near(length(&path), length(&whole) / 4.0);
    // From the top corner to the next one clockwise, or the other way round.
    let end = path.segments().last().unwrap().end();
    near(end.x, 50.0);
    near(end.y, 0.0);
    let back = one(&format!(
        r#"{{"star":4,"type":"polygon","outer-radius":50,"direction":"counterclockwise"}},{{"trim":0.25}},{STROKE}"#
    ));
    let end = back.segments().last().unwrap().end();
    near(end.x, -50.0);
    near(end.y, 0.0);
}

#[test]
fn start_after_end_swaps() {
    let a = one(&format!(r#"{LINE},{{"trim":0.25,"start":0.75}},{STROKE}"#));
    let b = one(&format!(r#"{LINE},{{"trim":0.75,"start":0.25}},{STROKE}"#));
    assert_eq!(a, b);
    near(length(&a), 50.0);
}

#[test]
fn an_offset_wraps_a_closed_outline_into_one_piece() {
    let path = one(&format!(
        r#"{{"rect":[100,40]}},{{"trim":0.5,"offset":270}},{STROKE}"#
    ));
    near(length(&path), 140.0);
    assert_eq!(pieces(&path), 1);
}

#[test]
fn an_offset_wraps_an_open_outline_into_two_pieces() {
    let path = one(&format!(r#"{LINE},{{"trim":0.5,"offset":270}},{STROKE}"#));
    near(length(&path), 50.0);
    assert_eq!(pieces(&path), 2);
}

#[test]
fn an_offset_of_whole_turns_changes_nothing() {
    let a = one(&format!(r#"{LINE},{{"trim":0.5,"offset":-720}},{STROKE}"#));
    let b = one(&format!(r#"{LINE},{{"trim":0.5}},{STROKE}"#));
    assert_eq!(a, b);
}

#[test]
fn an_empty_stretch_draws_nothing() {
    let all = draws(&format!(r#"{LINE},{{"trim":0.3,"start":0.3}},{STROKE}"#));
    assert!(all.is_empty(), "{all:?}");
}

#[test]
fn a_whole_stretch_keeps_the_outline_closed() {
    let path = one(&format!(
        r#"{{"rect":[100,40]}},{{"trim":1,"offset":90}},{STROKE}"#
    ));
    assert!(closed(&path));
    near(length(&path), 280.0);
}

#[test]
fn a_paint_before_the_trim_draws_the_cut_outline() {
    let path = one(&format!(r#"{LINE},{STROKE},{{"trim":0.5}}"#));
    near(length(&path), 50.0);
}

#[test]
fn an_outer_trim_cuts_an_inner_group_and_its_paints() {
    let all = draws(&format!(
        r#"{{"group":[{LINE},{STROKE}],"scale":[2,2]}},{STROKE},{{"trim":0.5}}"#
    ));
    assert_eq!(all.len(), 2);
    // Back to front: the outer stroke draws the cut line through the
    // group's scale, the inner one in the group's own space.
    near(length(&all[0].outline), 100.0);
    near(length(&all[1].outline), 50.0);
}

#[test]
fn an_inner_trim_cuts_before_an_outer_one() {
    let path = one(&format!(
        r#"{{"group":[{LINE},{{"trim":0.5}}]}},{{"trim":0.5}},{STROKE}"#
    ));
    near(length(&path), 25.0);
}

#[test]
fn joined_spreads_one_stretch_over_the_outlines_in_order() {
    let two = r#"{"path":"M0,0 L100,0"},{"path":"M0,10 L100,10"}"#;
    let joined = one(&format!(
        r#"{two},{{"trim":0.75,"mode":"joined"}},{STROKE}"#
    ));
    near(length(&joined), 150.0);
    assert_eq!(pieces(&joined), 2);
    let each = one(&format!(r#"{two},{{"trim":0.75}},{STROKE}"#));
    near(length(&each), 150.0);
    near(each.segments().next().unwrap().end().x, 75.0);
    near(joined.segments().next().unwrap().end().x, 100.0);
}

#[test]
fn joined_wraps_across_outlines_as_separate_pieces() {
    let two = r#"{"path":"M0,0 L100,0"},{"path":"M0,10 L100,10"}"#;
    let path = one(&format!(
        r#"{two},{{"trim":0.5,"offset":270,"mode":"joined"}},{STROKE}"#
    ));
    near(length(&path), 100.0);
    assert_eq!(pieces(&path), 2);
}
