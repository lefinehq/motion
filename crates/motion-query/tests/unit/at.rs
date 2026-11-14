//! Unit tests for `src/at.rs`.

use crate::at;
use motion_format::{Diagnostic, Kind};

#[test]
fn at_prints_resolved_values() {
    let s = motion_format::parse_document(include_str!("../../../../examples/launch.json"))
        .unwrap()
        .scene()
        .clone();
    let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
    assert_eq!(
        at(&g, 200.0, &["title".into()]).unwrap(),
        "title x=960 y=260 w=580.8 h=115.2"
    );
    assert_eq!(at(&g, 0.0, &[]).unwrap().lines().count(), 2);
    assert_eq!(
        at(&g, 0.0, &["nope".into()]).unwrap_err(),
        Diagnostic::unknown("layer id", "nope", ["title", "card"])
    );
    let empty = motion_scene::compile(
        motion_format::parse_document(r#"{"motion":1,"dur":"1s","layers":[]}"#)
            .unwrap()
            .scene(),
        &motion_scene::ApproxTextMeasure,
    );
    assert_eq!(at(&empty, 0.0, &[]).unwrap(), "");
}

/// Compiles a one-layer scene.
fn graph(layer: &str) -> motion_scene::SceneGraph {
    let json = format!(r#"{{"motion":1,"dur":"2s","layers":[{layer}]}}"#);
    let s = motion_format::parse_document(&json)
        .unwrap()
        .scene()
        .clone();
    motion_scene::compile(&s, &motion_scene::ApproxTextMeasure)
}

#[test]
fn non_default_properties_follow_the_fixed_columns() {
    let g =
        graph(r#"{"id":"a","rect":"100x100","at":"500 400","in":"fade-up 60px 10f linear @0f"}"#);
    // Frame 5: half faded in and half way up the 60 px entrance.
    assert_eq!(
        at(&g, 5.0, &[]).unwrap(),
        "a x=500 y=430 w=100 h=100 opacity=0.5"
    );
}

#[test]
fn properties_at_their_default_are_left_out() {
    let g = graph(r#"{"id":"a","rect":"100x100","at":"500 400","in":"pop 10f linear @0f"}"#);
    assert_eq!(at(&g, 10.0, &[]).unwrap(), "a x=500 y=400 w=100 h=100");
}

#[test]
fn pop_and_spin_in_mid_entrance() {
    let g = graph(
        r#"{"id":"a","rect":"100x100","at":"300 400","in":"pop 10f linear @0f"},{"id":"b","rect":"100x100","at":"700 400","in":"spin-in 10f linear @0f"}"#,
    );
    insta::assert_snapshot!(at(&g, 5.0, &[]).unwrap());
}

#[test]
fn errors_say_what_kind_of_problem_they_are() {
    let g = graph(r#"{"id":"a","rect":"10x10"}"#);
    assert_eq!(at(&g, -1.0, &[]).unwrap_err().kind, Kind::OutOfRange);
    assert_eq!(
        at(&g, 0.0, &["ghost".to_string()]).unwrap_err(),
        Diagnostic::unknown("layer id", "ghost", ["a"])
    );
}

#[test]
fn a_child_prints_where_its_anchor_lands_on_the_canvas() {
    let s = motion_format::parse_document(
        r#"{"motion":1,"dur":"1s","layers":[
        {"id":"card","rect":"200x100","at":"500 400","scale":2},
        {"id":"kid","rect":"10x10","parent":"card","at":"center center"}]}"#,
    )
    .unwrap()
    .scene()
    .clone();
    let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
    // `center center` is the middle of the card's box, which sits at (500, 400).
    assert_eq!(
        at(&g, 0.0, &["kid".into()]).unwrap(),
        "kid x=500 y=400 w=10 h=10"
    );
}
