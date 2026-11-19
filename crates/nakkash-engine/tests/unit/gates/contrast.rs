//! Unit tests for `src/gates/contrast.rs`.

use super::*;
use pretty_assertions::assert_eq;
use std::str::FromStr;

/// An image of `px`, straight RGBA, one row.
fn row(px: &[[u8; 4]]) -> Image {
    Image::new(px.len() as u32, 1, px.concat()).expect("image")
}

/// Nothing drawn over, the size of `like`.
fn clear(like: &Image) -> Image {
    let n = (like.width() * like.height()) as usize;
    Image::new(like.width(), like.height(), vec![0; n * 4]).expect("image")
}

/// [`ratio`] of `glyphs` laid over `behind`, as a text blended normally.
fn read(glyphs: &Image, behind: &Image) -> Option<f64> {
    let whole: Vec<u8> = glyphs
        .pixels()
        .chunks_exact(4)
        .zip(behind.pixels().chunks_exact(4))
        .flat_map(|(g, b)| {
            let a = f64::from(g[3]) / 255.0;
            let mix = |c: usize| (f64::from(g[c]) * a + f64::from(b[c]) * (1.0 - a)).round() as u8;
            [mix(0), mix(1), mix(2), 255]
        })
        .collect();
    let whole = Image::new(glyphs.width(), glyphs.height(), whole).expect("image");
    ratio(glyphs, behind, &whole, &clear(glyphs))
}

#[test]
fn a_text_reads_as_the_frame_shows_it() {
    // White glyphs that a template's compositions blend over a mid-grey
    // frame so that they lift it only a little read near 1:1, as they
    // show, not as the white they were set in.
    let glyphs = row(&[[255, 255, 255, 255], [255, 255, 255, 255], [0, 0, 0, 0]]);
    let behind = row(&[[128, 128, 128, 255]; 3]);
    let whole = row(&[
        [150, 150, 150, 255],
        [150, 150, 150, 255],
        [128, 128, 128, 255],
    ]);
    let ratio = ratio(&glyphs, &behind, &whole, &clear(&glyphs)).unwrap();
    assert!(ratio < 1.6, "{ratio}");
}

#[test]
fn white_on_black_is_21_to_1() {
    let glyphs = row(&[[255, 255, 255, 255], [0, 0, 0, 0]]);
    let behind = row(&[[0, 0, 0, 255], [0, 0, 0, 255]]);
    assert_eq!(read(&glyphs, &behind), Some(21.0));
}

#[test]
fn the_worst_tenth_of_the_coverage_may_fall_short() {
    // Nine px of white on black, one on white.
    let glyphs = row(&[[255, 255, 255, 255]; 10]);
    let mut back = [[0, 0, 0, 255]; 10];
    back[0] = [255, 255, 255, 255];
    assert_eq!(read(&glyphs, &row(&back)), Some(1.0));
    back[1] = [0, 0, 0, 255];
    let behind = row(&back[1..]);
    assert_eq!(read(&row(&[[255, 255, 255, 255]; 9]), &behind), Some(21.0));
}

#[test]
fn glyphs_covering_less_than_a_px_are_not_measured() {
    let glyphs = row(&[[255, 255, 255, 100], [255, 255, 255, 100]]);
    let behind = row(&[[0, 0, 0, 255], [0, 0, 0, 255]]);
    assert_eq!(read(&glyphs, &behind), None);
}

/// A scene of one title, `fill` on `bg`, at `size` px.
fn titled(fill: &str, bg: &str, size: u32) -> String {
    let report = check(
        &Session::from_str(&format!(
            r##"{{"nakkash":1,"size":"1080p","dur":"1s","bg":"{bg}","layers":[
                {{"id":"title","text":"Launch Week","font":"Inter Variable {size} 700","fill":"{fill}","at":"center center"}}]}}"##
        ))
        .expect("scene"),
        false,
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
        r##"{"nakkash":1,"dur":"1s","layers":[{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10"}]}"##,
    )
    .expect("scene");
    let state = session.frame(0.0).expect("frame");
    let hidden = |s: &FrameState| s.nodes.iter().map(|n| n.hidden).collect::<Vec<_>>();
    assert_eq!(hidden(&without(&state, &[vec!["a"]])), [true, false]);
    assert_eq!(hidden(&alone(&state, &[vec!["a"]])), [false, true]);
    // Two paths keep both, or hide both.
    assert_eq!(
        hidden(&alone(&state, &[vec!["a"], vec!["b"]])),
        [false, false]
    );
    assert_eq!(
        hidden(&without(&state, &[vec!["a"], vec!["b"]])),
        [true, true]
    );
}

/// A title drawn as `copies` stacked copies, a px apart as a split or an
/// echo stacks them, `fill` on `bg`.
fn stacked(fill: &str, bg: &str, copies: usize) -> String {
    let layers: Vec<String> = (0..copies)
        .map(|i| {
            format!(
                r##"{{"id":"title-{i}","text":"Launch Week","font":"Inter Variable 96 700","fill":"{fill}","position":[{},540]}}"##,
                400 + i
            )
        })
        .collect();
    let report = check(
        &Session::from_str(&format!(
            r##"{{"nakkash":1,"size":"1080p","dur":"1s","bg":"{bg}","layers":[{}]}}"##,
            layers.join(",")
        ))
        .expect("scene"),
        false,
    )
    .expect("contrast");
    report.to_string()
}

#[test]
fn copies_of_one_text_are_read_together_against_what_is_behind_them() {
    // Each copy alone would read 1:1 against the others.
    assert_eq!(stacked("#FFFFFF", "#000000", 3), "OK 3");
    // And a muddy title still is one, however many copies draw it.
    assert_eq!(
        stacked("#777777", "#666666", 3),
        "E contrast title-0 0-15f 1.2:1 <3\nE contrast title-1 0-15f 1.2:1 <3\nE contrast title-2 0-15f 1.2:1 <3\nOK 0"
    );
}

#[test]
fn instances_of_one_composition_are_copies_wherever_they_are_moved() {
    // Two instances of one title composition, the back one moved up by
    // most of a line, as a split or a glitch moves it: one white title
    // over black. Alone, the back one reads 1:1 where the front one
    // covers it in its own colour.
    let scene = r##"{"nakkash":1,"size":"1080p","dur":"1s","bg":"#000000",
        "comps":{"word":{"size":"1920x1080","dur":"1s","layers":[
            {"id":"t","text":"NAKKASH","font":"Inter Variable 120 800","fill":"#FFFFFF","at":"center center"}]}},
        "layers":[
            {"id":"c-1","comp":"word","position":[960,470]},
            {"id":"c","comp":"word"}]}"##;
    let report = check(&Session::from_str(scene).expect("scene"), false).expect("contrast");
    assert_eq!(report.to_string(), "OK 2");
    assert!(instances(
        &["c-2".into(), "t".into()],
        &["c".into(), "t".into()]
    ));
    assert!(!instances(
        &["a".into(), "t".into()],
        &["a".into(), "u".into()]
    ));
}

/// A scene whose one layer is a composition with `bg`, holding a title in
/// `fill`, over a scene `bg` of off-white.
fn titled_in_comp(fill: &str, bg: &str) -> String {
    let report = check(
        &Session::from_str(&format!(
            r##"{{"nakkash":1,"size":"1920x1080","fps":30,"dur":"2s","bg":"#f5f4f0",
            "comps":{{"a":{{"size":"1920x1080","dur":"2s","bg":"{bg}","layers":[
                {{"id":"h","text":"Hello there","font":"Inter Variable 96 500","fill":"{fill}","at":"center center"}}]}}}},
            "layers":[{{"id":"a","comp":"a"}}]}}"##
        ))
        .expect("scene"),
        false,
    )
    .expect("contrast");
    report.to_string()
}

#[test]
fn a_composition_background_is_behind_its_text_not_its_glyphs() {
    // Ink on yellow, about 13:1: the yellow drawn across the composition
    // read as glyphs on yellow, 1:1.
    assert_eq!(titled_in_comp("#0b0b0b", "#ffd23f"), "OK 1");
}

#[test]
fn text_on_a_composition_background_of_its_own_tone_is_an_error() {
    assert_eq!(
        titled_in_comp("#111111", "#0b0b0b"),
        "E contrast a/h 0-45f 1:1 <3\nOK 0"
    );
}

/// Texts: one plain, one too faint to read, one clipped to a sliver by a
/// matte, one scaled to nothing, so some glyphs cover under a px.
const SOME_UNSEEN: &str = r##"{"nakkash":1,"size":"640x360","fps":4,"dur":"2s","bg":"#202020","layers":[
    {"id":"plain","text":"Plain","font":"Inter Variable 40","fill":"#FFFFFF","at":"center 80"},
    {"id":"cut","rect":"1x1","at":"10 10"},
    {"id":"thin","text":"Clipped","font":"Inter Variable 40","fill":"#303030","at":"10 10","matte":"cut"},
    {"id":"tiny","text":"Gone","font":"Inter Variable 40","fill":"#222222","at":"center 250","scale":0.001}]}"##;

#[test]
fn passing_over_glyphs_under_a_px_changes_nothing() {
    let session = Session::from_str(SOME_UNSEEN).expect("scene");
    let drawn = check(&session, false).expect("contrast").to_string();
    assert_eq!(check(&session, true).expect("contrast").to_string(), drawn);
}

#[test]
fn a_glyph_counts_only_where_the_text_is_drawn() {
    // The shape covers two px; a wipe has taken the second away.
    let shape = row(&[[255, 255, 255, 255], [255, 255, 255, 255]]);
    let drawn = row(&[[255, 0, 0, 255], [0, 0, 0, 0]]);
    let glyphs = reached(&shape, &drawn).unwrap();
    assert_eq!(glyphs.pixels(), &[255, 255, 255, 255, 255, 255, 255, 0]);
}

#[test]
fn a_text_something_covers_is_not_read_where_it_is_covered() {
    // Dark glyphs on white: a grey card drawn over the first two px leaves
    // the frame as it is behind there; the third shows.
    let glyphs = row(&[[0, 0, 0, 255]; 3]);
    let behind = row(&[
        [220, 220, 220, 255],
        [220, 220, 220, 255],
        [255, 255, 255, 255],
    ]);
    let card = row(&[[220, 220, 220, 255], [220, 220, 220, 255], [0, 0, 0, 0]]);
    let whole = row(&[[220, 220, 220, 255], [220, 220, 220, 255], [0, 0, 0, 255]]);
    assert_eq!(ratio(&glyphs, &behind, &whole, &card), Some(21.0));
    // Covered whole, it is not read.
    let whole = row(&[
        [220, 220, 220, 255],
        [220, 220, 220, 255],
        [255, 255, 255, 255],
    ]);
    let card = row(&[[220, 220, 220, 255]; 3]);
    assert_eq!(ratio(&glyphs, &behind, &whole, &card), None);
}

#[test]
fn a_text_nothing_covers_reads_1_to_1_where_the_frame_does_not_show_it() {
    // White glyphs whose compositions draw nothing where they are (moved
    // away, matted out by a blend): the frame is what is behind, and with
    // nothing over them they read 1:1, as they show.
    let glyphs = row(&[[255, 255, 255, 255]; 2]);
    let behind = row(&[[0, 0, 0, 255]; 2]);
    let whole = row(&[[0, 0, 0, 255]; 2]);
    assert_eq!(ratio(&glyphs, &behind, &whole, &clear(&glyphs)), Some(1.0));
}

#[test]
fn a_text_set_in_the_colour_behind_it_still_reads_1_to_1() {
    // The frame does not change, and the glyphs would not stand out there
    // either: a faint text, read even under a card.
    let glyphs = row(&[[250, 250, 250, 255]; 2]);
    let behind = row(&[[255, 255, 255, 255]; 2]);
    let whole = row(&[[255, 255, 255, 255]; 2]);
    let card = row(&[[255, 255, 255, 255]; 2]);
    let r = ratio(&glyphs, &behind, &whole, &card).unwrap();
    assert!(r < 1.1, "{r}");
}
