//! Unit tests for `src/space.rs`, and 3D nodes and cameras as evaluation
//! places them.

use super::*;
use crate::{SceneGraph, evaluate};
use rstest::rstest;

fn graph(json: &str) -> SceneGraph {
    crate::compile(
        motion_format::parse_document(json).unwrap().scene(),
        &mut motion_text::Typesetter::new(motion_text::FontSet::builtin()),
    )
    .unwrap()
}

/// The canvas point the plane point `p` of node `i` lands on at `frame`.
fn landed(graph: &SceneGraph, frame: f64, i: usize, p: (f64, f64)) -> Option<Point> {
    let state = evaluate(graph, frame);
    let plane = state.view.plane(state.nodes[i].space.expect("a 3D node"));
    project(plane, Point::new(p.0, p.1))
}

fn close(a: Point, b: (f64, f64)) {
    assert!(
        (a.x - b.0).abs() < 1e-6 && (a.y - b.1).abs() < 1e-6,
        "{a:?} is not {b:?}"
    );
}

/// A 200x100 card on a 1920x1080 canvas, with `extra` keys on the card and
/// `layers` before it.
fn card(extra: &str, layers: &str) -> SceneGraph {
    graph(&format!(
        r#"{{"motion":1,"size":"1920x1080","dur":"1s","layers":[{layers}{{"id":"card","rect":"200x100","3d":true{extra}}}]}}"#
    ))
}

#[test]
fn the_default_camera_shows_depth_zero_as_a_flat_layer_would() {
    let g = card("", "");
    // The card is centred, so its corner sits 100 and 50 px from the centre.
    close(landed(&g, 0.0, 0, (0.0, 0.0)).unwrap(), (860.0, 490.0));
    close(landed(&g, 0.0, 0, (200.0, 100.0)).unwrap(), (1060.0, 590.0));
}

#[test]
fn the_default_zoom_is_a_39_6_degree_view() {
    let view = View::default_for(1920.0, 1080.0);
    assert!((view.zoom - 2_666.666_666).abs() < 1e-3, "{}", view.zoom);
}

#[test]
fn a_plane_one_zoom_away_shows_at_half_size_about_the_centre() {
    let zoom = View::default_for(1920.0, 1080.0).zoom;
    let g = card(&format!(r#","z":{zoom}"#), "");
    close(landed(&g, 0.0, 0, (0.0, 0.0)).unwrap(), (910.0, 515.0));
    close(landed(&g, 0.0, 0, (200.0, 100.0)).unwrap(), (1010.0, 565.0));
}

#[rstest]
#[case::about_y(r#","rotate-y":90"#)]
#[case::about_x(r#","rotate-x":90"#)]
#[case::by_orientation(r#","orient":"0,90,0""#)]
fn a_quarter_turn_shows_the_plane_edge_on(#[case] turn: &str) {
    let g = card(turn, "");
    let a = landed(&g, 0.0, 0, (0.0, 0.0)).unwrap();
    let b = landed(&g, 0.0, 0, (200.0, 100.0)).unwrap();
    // Edge-on, the corners line up on one line through the centre.
    let cross = (a.x - 960.0) * (b.y - 540.0) - (a.y - 540.0) * (b.x - 960.0);
    assert!(cross.abs() < 1e-6, "{a:?} {b:?}");
}

#[test]
fn turning_about_y_brings_the_right_edge_closer() {
    // Turned 60° about Y (down), clockwise seen from above, the right edge
    // comes closer and the left leaves, so the right edge shows taller.
    let g = card(r#","rotate-y":60"#, "");
    let top_left = landed(&g, 0.0, 0, (0.0, 0.0)).unwrap();
    let bottom_left = landed(&g, 0.0, 0, (0.0, 100.0)).unwrap();
    let top_right = landed(&g, 0.0, 0, (200.0, 0.0)).unwrap();
    let bottom_right = landed(&g, 0.0, 0, (200.0, 100.0)).unwrap();
    let (left, right) = (bottom_left.y - top_left.y, bottom_right.y - top_right.y);
    assert!(left < 100.0 && right > 100.0, "{left} {right}");
    // Foreshortened to about cos 60° of its width.
    let width = top_right.x - top_left.x;
    assert!((width - 100.0).abs() < 6.0, "{width}");
}

#[test]
fn a_plane_behind_the_camera_does_not_land() {
    let g = card(r#","z":-3000"#, "");
    assert_eq!(landed(&g, 0.0, 0, (0.0, 0.0)), None);
}

#[test]
fn a_camera_with_the_default_place_sees_as_the_default_camera() {
    let g = card(r#""#, r#"{"id":"cam","camera":true},"#);
    close(landed(&g, 0.0, 1, (0.0, 0.0)).unwrap(), (860.0, 490.0));
}

#[test]
fn a_camera_aimed_at_the_centre_from_the_default_place_matches() {
    let g = card(
        r#""#,
        r#"{"id":"cam","camera":true,"target":[960,540],"target-z":0},"#,
    );
    close(landed(&g, 0.0, 1, (0.0, 0.0)).unwrap(), (860.0, 490.0));
}

#[test]
fn a_camera_that_dollies_in_magnifies() {
    let g = card(
        r#""#,
        r#"{"id":"cam","camera":true,"z":-1333.3333333333333},"#,
    );
    let a = landed(&g, 0.0, 1, (0.0, 0.0)).unwrap();
    let b = landed(&g, 0.0, 1, (200.0, 100.0)).unwrap();
    assert!(((b.x - a.x) - 400.0).abs() < 0.1, "{a:?} {b:?}");
}

#[test]
fn a_camera_moved_right_sees_the_card_move_left() {
    let g = card(r#""#, r#"{"id":"cam","camera":true,"at":"1060 540"},"#);
    close(landed(&g, 0.0, 1, (0.0, 0.0)).unwrap(), (760.0, 490.0));
}

#[test]
fn the_last_camera_that_shows_is_used() {
    let g = card(
        r#""#,
        r#"{"id":"near","camera":true,"z":-2000},{"id":"moved","camera":true,"at":"1060 540","span":"0.5s-1s"},"#,
    );
    // Before the second shows, the first is used; then the second.
    let a = landed(&g, 0.0, 2, (0.0, 0.0)).unwrap();
    assert!((a.x - 760.0).abs() > 1.0);
    close(landed(&g, 20.0, 2, (0.0, 0.0)).unwrap(), (760.0, 490.0));
}

#[test]
fn a_3d_child_moves_with_its_3d_parent() {
    let g = graph(
        r#"{"motion":1,"size":"1920x1080","dur":"1s","layers":[
        {"id":"rig","pivot":true,"3d":true,"z":500},
        {"id":"card","rect":"200x100","3d":true,"parent":"rig","at":"0 0"}]}"#,
    );
    let state = evaluate(&g, 0.0);
    let z = state.nodes[1]
        .space
        .unwrap()
        .transform_point3(DVec3::new(100.0, 50.0, 0.0))
        .z;
    assert!((z - 500.0).abs() < 1e-9, "{z}");
}

#[test]
fn a_flat_child_of_a_3d_parent_sees_it_straight_on() {
    let g = graph(
        r#"{"motion":1,"size":"1920x1080","dur":"1s","layers":[
        {"id":"rig","pivot":true,"3d":true,"z":500,"at":"100 0"},
        {"id":"card","rect":"200x100","parent":"rig","at":"0 0"}]}"#,
    );
    let state = evaluate(&g, 0.0);
    assert_eq!(state.nodes[1].space, None);
    let corner = state.nodes[1].transform * Point::ZERO;
    close(corner, (0.0, -50.0));
}

#[rstest]
#[case([0.0, 0.0, 0.0])]
#[case([10.0, 20.0, 30.0])]
#[case([-45.0, 80.0, 170.0])]
fn angles_read_back_the_same_turn(#[case] given: [f64; 3]) {
    let turn = orientation(given);
    let again = orientation(angles(turn));
    // `angle_between` reads an arc cosine, so 1e-8 rad is its noise.
    assert!(turn.angle_between(again) < 1e-6, "{given:?}");
}

#[test]
fn orientation_keys_turn_the_shortest_way() {
    let g = card(r#","anim":{"orient":"0,350,0 -> 0,10,0 @0-10f"}"#, "");
    let state = evaluate(&g, 5.0);
    let half = orientation(
        state.nodes[0].values[Prop::ORIENT.index()]
            .as_orientation()
            .unwrap(),
    );
    // Halfway, the turn is at 0 about Y, not at 180.
    assert!(half.angle_between(DQuat::IDENTITY) < 1e-6);
}

#[test]
fn orientation_keys_ease_over_the_angle() {
    let g = card(r#","anim":{"orient":"0,0,0 -> 0,90,0 @0-10f"}"#, "");
    let state = evaluate(&g, 5.0);
    let half = orientation(
        state.nodes[0].values[Prop::ORIENT.index()]
            .as_orientation()
            .unwrap(),
    );
    assert!((half.angle_between(DQuat::IDENTITY).to_degrees() - 45.0).abs() < 1e-6);
}

#[test]
fn flattening_a_lifted_transform_gives_it_back() {
    let transform = Affine::new([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    assert_eq!(flatten(lift(transform)), transform);
}

/// The corners of a `width` by `height` rectangle centred on `at`, turned
/// by `turn` about Y, in the camera's space of a camera at the origin.
fn plate(at: DVec3, width: f64, height: f64, turn: f64) -> [DVec3; 4] {
    let turned = DMat4::from_translation(at) * DMat4::from_rotation_y(turn.to_radians());
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(x, y)| turned.transform_point3(DVec3::new(x * width / 2.0, y * height / 2.0, 0.0)))
}

#[test]
fn a_plate_wholly_on_the_cameras_side_of_another_is_in_front() {
    use std::cmp::Ordering;
    let card = plate(DVec3::new(0.0, 0.0, 1000.0), 400.0, 100.0, 50.0);
    // 1 px in front of the card, near its edge that turns away: its centre
    // is deeper than the card's.
    let along = DVec3::new(50f64.to_radians().cos(), 0.0, -50f64.to_radians().sin());
    let towards = DVec3::new(-50f64.to_radians().sin(), 0.0, -50f64.to_radians().cos());
    let label = plate(
        DVec3::new(0.0, 0.0, 1000.0) - along * 170.0 + towards,
        20.0,
        20.0,
        50.0,
    );
    let centre = |c: &[DVec3; 4]| (c[0] + c[2]).z / 2.0;
    assert!(
        centre(&label) > centre(&card),
        "{} {}",
        centre(&label),
        centre(&card)
    );
    assert_eq!(depth_order(&card, &label), Some(Ordering::Less));
    assert_eq!(depth_order(&label, &card), Some(Ordering::Greater));
}

#[test]
fn plates_in_one_plane_are_equal_and_crossing_ones_undecided() {
    use std::cmp::Ordering;
    let a = plate(DVec3::new(0.0, 0.0, 1000.0), 400.0, 100.0, 30.0);
    let b = plate(DVec3::new(0.0, 20.0, 1000.0), 50.0, 50.0, 30.0);
    assert_eq!(depth_order(&a, &b), Some(Ordering::Equal));
    let crossing = plate(DVec3::new(0.0, 0.0, 1000.0), 400.0, 100.0, -30.0);
    assert_eq!(depth_order(&a, &crossing), None);
}
