//! Unit tests for `src/repeater.rs`: where each copy stands, how it fades,
//! the order copies draw in and what the items after a repeater reach.

use super::*;
use crate::{ContentState, ShapeDraw, compile, evaluate};
use kurbo::{Point, Rect, Shape};
use motion_format::parse_document;
use motion_text::{FontSet, Typesetter};

/// The frame-0 draws of a scene whose one layer holds `shapes`, back to
/// front.
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

/// The draws of a layer `{square, fill, repeater}` whose repeater holds
/// `copies` and the keys `rest`.
fn square(copies: f64, rest: &str) -> Vec<ShapeDraw> {
    draws(&format!(
        r##"{{"rect":[10,10]}},{{"fill":"#FFFFFF"}},{{"repeater":{copies}{rest}}}"##
    ))
}

/// Where a draw's outline lands in the layer: the middle of its box.
fn centre(draw: &ShapeDraw) -> Point {
    draw.transform
        .transform_rect_bbox(draw.outline.bounding_box())
        .center()
}

fn near(a: Point, b: Point) {
    assert!((a - b).hypot() < 1e-6, "{a:?} vs {b:?}");
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} vs {b}");
}

#[test]
fn copies_stand_a_position_apart_and_the_first_stays_in_place() {
    let d = square(3.0, r#","position":[20,5]"#);
    assert_eq!(d.len(), 3);
    // Back to front: with `below`, the first copy is drawn last, on top.
    near(centre(&d[2]), Point::new(0.0, 0.0));
    near(centre(&d[1]), Point::new(20.0, 5.0));
    near(centre(&d[0]), Point::new(40.0, 10.0));
}

#[test]
fn above_draws_each_copy_over_the_one_before() {
    let d = square(3.0, r#","position":[20,0],"composite":"above""#);
    near(centre(&d[0]), Point::new(0.0, 0.0));
    near(centre(&d[2]), Point::new(40.0, 0.0));
}

#[test]
fn the_default_steps_a_hundred_px_to_the_right() {
    let d = square(3.0, "");
    near(centre(&d[0]), Point::new(200.0, 0.0));
}

#[test]
fn turns_add_up_about_the_anchor_without_turning_the_moves() {
    // Copies turned 90° a step about (0, 50), moved 10 px right a step.
    let d = square(3.0, r#","position":[10,0],"anchor":[0,50],"rotate":90"#);
    near(centre(&d[2]), Point::new(0.0, 0.0));
    // (0, 0) turned 90° clockwise about (0, 50) is (50, 50), then moved.
    near(centre(&d[1]), Point::new(60.0, 50.0));
    near(centre(&d[0]), Point::new(20.0, 100.0));
}

#[test]
fn scale_is_a_power_of_the_step_about_the_anchor() {
    let d = square(3.0, r#","position":[0,0],"anchor":[100,0],"scale":[2,0.5]"#);
    let size = |draw: &ShapeDraw| {
        let r = draw
            .transform
            .transform_rect_bbox(draw.outline.bounding_box());
        (r.width(), r.height())
    };
    assert_eq!(size(&d[2]), (10.0, 10.0));
    assert_eq!(size(&d[1]), (20.0, 5.0));
    assert_eq!(size(&d[0]), (40.0, 2.5));
    // The anchor stays where it is: (0, 0) moves away from it.
    near(centre(&d[0]), Point::new(100.0 - 4.0 * 100.0, 0.0));
}

#[test]
fn a_negative_scale_mirrors_every_other_copy() {
    close(power(-0.5, 0.0), 1.0);
    close(power(-0.5, 1.0), -0.5);
    close(power(-0.5, 2.0), 0.25);
    close(power(2.0, 0.5), 2f64.sqrt());
}

#[test]
fn opacity_steps_a_copies_th_of_the_way_from_start_to_end() {
    let d = square(3.0, r#","start-opacity":1,"end-opacity":0.4"#);
    close(d[2].opacity, 1.0);
    close(d[1].opacity, 0.8);
    close(d[0].opacity, 0.6);
}

#[test]
fn a_fractional_count_draws_one_more_copy_faded_by_the_fraction() {
    let d = square(2.25, r#","position":[10,0]"#);
    assert_eq!(d.len(), 3);
    near(centre(&d[0]), Point::new(20.0, 0.0));
    close(d[0].opacity, 0.25);
    close(d[1].opacity, 1.0);
}

#[test]
fn no_copies_draw_nothing() {
    assert!(square(0.0, "").is_empty());
    assert!(square(0.5, "").len() == 1);
}

#[test]
fn an_offset_moves_every_copy_along_fractional_and_negative_alike() {
    let d = square(3.0, r#","offset":-1.5,"position":[10,0]"#);
    near(centre(&d[2]), Point::new(-15.0, 0.0));
    near(centre(&d[0]), Point::new(5.0, 0.0));
    // The offset does not change the fade.
    close(d[2].opacity, 1.0);
}

#[test]
fn a_paint_after_the_repeater_draws_every_copy_once_unfaded() {
    let d = draws(
        r##"{"rect":[10,10]},{"repeater":3,"position":[20,0],"end-opacity":0},{"fill":"#FFFFFF"}"##,
    );
    assert_eq!(d.len(), 1);
    close(d[0].opacity, 1.0);
    assert_eq!(
        d[0].outline.bounding_box(),
        Rect::new(-5.0, -5.0, 45.0, 5.0)
    );
}

#[test]
fn a_trim_after_the_repeater_cuts_every_copy() {
    let d = draws(
        r##"{"path":"M0,0 L100,0"},{"stroke":"#FFFFFF"},{"repeater":2,"position":[0,10]},{"trim":0.5}"##,
    );
    assert_eq!(d.len(), 2);
    for draw in &d {
        let b = draw.outline.bounding_box();
        close(b.x0, 0.0);
        assert!((b.x1 - 50.0).abs() < 1e-9, "{b:?}");
    }
}

#[test]
fn a_second_repeater_copies_the_first_ones_copies() {
    let d = square(
        3.0,
        r#","position":[10,0]},{"repeater":2,"position":[0,10]"#,
    );
    assert_eq!(d.len(), 6);
    let mut at: Vec<(i64, i64)> = d
        .iter()
        .map(|draw| {
            let c = centre(draw);
            (c.x.round() as i64, c.y.round() as i64)
        })
        .collect();
    at.sort_unstable();
    assert_eq!(at, [(0, 0), (0, 10), (10, 0), (10, 10), (20, 0), (20, 10)]);
}

#[test]
fn copies_carry_their_group_and_the_groups_inside() {
    let d = draws(
        r##"{"group":[{"rect":[10,10]},{"fill":"#FFFFFF"}],"position":[0,30]},{"repeater":2,"position":[20,0],"scale":2}"##,
    );
    near(centre(&d[1]), Point::new(0.0, 30.0));
    // The second copy is scaled about the anchor, its group's move with it.
    near(centre(&d[0]), Point::new(20.0, 60.0));
}
