//! Unit tests for `src/gates/contrast.rs`.

use super::*;
use pretty_assertions::assert_eq;
use std::str::FromStr;

/// An image of `px`, straight RGBA, one row.
fn row(px: &[[u8; 4]]) -> Image {
    Image::new(px.len() as u32, 1, px.concat()).expect("image")
}

#[test]
fn white_on_black_is_21_to_1() {
    let glyphs = row(&[[255, 255, 255, 255], [0, 0, 0, 0]]);
    let behind = row(&[[0, 0, 0, 255], [0, 0, 0, 255]]);
    assert_eq!(ratio(&glyphs, &behind), Some(21.0));
}

#[test]
fn the_worst_tenth_of_the_coverage_may_fall_short() {
    // Nine px of white on black, one on white.
    let glyphs = row(&[[255, 255, 255, 255]; 10]);
    let mut back = [[0, 0, 0, 255]; 10];
    back[0] = [255, 255, 255, 255];
    assert_eq!(ratio(&glyphs, &row(&back)), Some(1.0));
    back[1] = [0, 0, 0, 255];
    let behind = row(&back[1..]);
    assert_eq!(ratio(&row(&[[255, 255, 255, 255]; 9]), &behind), Some(21.0));
}

#[test]
fn glyphs_covering_less_than_a_px_are_not_measured() {
    let glyphs = row(&[[255, 255, 255, 100], [255, 255, 255, 100]]);
    let behind = row(&[[0, 0, 0, 255], [0, 0, 0, 255]]);
    assert_eq!(ratio(&glyphs, &behind), None);
}

/// A scene of one title, `fill` on `bg`, at `size` px.
fn titled(fill: &str, bg: &str, size: u32) -> String {
    let report = check(
        &Session::from_str(&format!(
            r##"{{"motion":1,"size":"1080p","dur":"1s","bg":"{bg}","layers":[
                {{"id":"title","text":"Launch Week","font":"Inter Variable {size} 700","fill":"{fill}","at":"center center"}}]}}"##
        ))
        .expect("scene"),
    )
    .expect("contrast");
    report.to_string()
}

#[test]
fn grey_on_grey_is_an_error_and_white_on_black_passes() {
    assert_eq!(
        titled("#777777", "#666666", 96),
        "E contrast title 0-15f 1.2:1 <3\nOK 0"
    );
    assert_eq!(titled("#FFFFFF", "#000000", 96), "OK 1");
}

#[test]
fn small_text_needs_more_contrast() {
    // 4:1, enough for large text only.
    assert_eq!(titled("#FFFFFF", "#7F7F7F", 96), "OK 1");
    assert_eq!(
        titled("#FFFFFF", "#7F7F7F", 20),
        "E contrast title 0-15f 4:1 <4.5\nOK 0"
    );
}

#[test]
fn what_is_behind_leaves_out_the_layer_and_alone_keeps_only_it() {
    let session = Session::from_str(
        r##"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10"}]}"##,
    )
    .expect("scene");
    let state = session.frame(0.0).expect("frame");
    let hidden = |s: &FrameState| s.nodes.iter().map(|n| n.hidden).collect::<Vec<_>>();
    assert_eq!(hidden(&without(&state, &["a"])), [true, false]);
    assert_eq!(hidden(&alone(&state, &["a"])), [false, true]);
}
