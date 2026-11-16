//! Unit tests for `src/rect.rs`: what a layer's content covers, as an
//! imported `sourceRectAtTime` reads it.

use super::*;
use crate::compile;
use motion_format::parse_document;
use motion_text::{FontSet, Typesetter};

fn graph(layers: &str) -> SceneGraph {
    let json = format!(
        r#"{{"motion":1,"dur":"2s","fps":10,
          "comps":{{"card":{{"size":"200x100","dur":"2s","layers":[{{"id":"inner","rect":"30x20"}}]}}}},
          "layers":[{layers}]}}"#
    );
    let doc = parse_document(&json).unwrap();
    compile(doc.scene(), &mut Typesetter::new(FontSet::builtin())).unwrap()
}

/// The rectangle of the scene's layer `id` at 0 s.
fn rect_of(layers: &str, id: &str, extents: bool) -> Rect {
    let address = Address::layer([id.parse().unwrap()]);
    content_rects(&graph(layers), &[(address, 0.0, extents)])[0].expect("the layer is there")
}

fn text(keys: &str) -> Rect {
    rect_of(&format!(r#"{{"id":"t",{keys}}}"#), "t", false)
}

#[test]
fn text_covers_the_outlines_of_its_glyphs() {
    let cap = text(r#""text":"H""#);
    let small = text(r#""text":"x""#);
    let descender = text(r#""text":"Hy""#);
    // Point text sets its first baseline at the origin: ink sits above it.
    assert!(cap.y0 < 0.0 && cap.y1.abs() < 0.5, "{cap:?}");
    // A lower-case letter is lower than a capital, and a descender reaches
    // below the baseline: the font's ascent and descent do not count.
    assert!(small.y0 > cap.y0 + 1.0, "{small:?} {cap:?}");
    assert!(descender.y1 > cap.y1 + 1.0, "{descender:?}");
    assert_eq!(descender.y0, cap.y0);
    // The left side bearing is not covered.
    assert!(cap.x0 > 0.0);
}

#[test]
fn trailing_spaces_cover_nothing() {
    assert_eq!(text(r#""text":"Hi   ""#), text(r#""text":"Hi""#));
}

#[test]
fn lines_add_up() {
    let one = text(r#""text":"Hi""#);
    let two = text(r#""text":"Hi\nHi""#);
    assert_eq!(two.y0, one.y0);
    assert!(two.height() > 2.0 * one.height(), "{two:?}");
}

#[test]
fn empty_text_covers_an_empty_rectangle_at_the_origin() {
    assert_eq!(text(r#""text":"""#), Rect::ZERO);
    assert_eq!(text(r#""text":"   ""#), Rect::ZERO);
}

#[test]
fn animators_change_nothing() {
    let still = text(r#""text":"Hey""#);
    let moved = text(
        r#""text":"Hey","animate":[{"animator":[{"range":"char","end":0.5}],"position":[0,100],"opacity":0,"scale":3}]"#,
    );
    assert_eq!(moved, still);
}

#[test]
fn box_text_covers_its_words_and_with_extents_its_box() {
    let layer = r#"{"id":"t","text":"Hey","box":[-50,-20,400,300]}"#;
    let words = rect_of(layer, "t", false);
    assert!(words.width() < 100.0 && words.x0 >= -50.0, "{words:?}");
    assert_eq!(
        rect_of(layer, "t", true),
        Rect::new(-50.0, -20.0, 350.0, 280.0)
    );
    // Point text has no box: extents change nothing.
    let point = r#"{"id":"t","text":"Hey"}"#;
    assert_eq!(rect_of(point, "t", true), rect_of(point, "t", false));
}

#[test]
fn shapes_cover_their_outlines_and_with_extents_their_strokes() {
    let layer = r##"{"id":"s","shapes":[{"rect":[10,10],"position":[5,0]},{"stroke":"#000000","width":4}]}"##;
    assert_eq!(rect_of(layer, "s", false), Rect::new(0.0, -5.0, 10.0, 5.0));
    assert_eq!(rect_of(layer, "s", true), Rect::new(-2.0, -7.0, 12.0, 7.0));
}

#[test]
fn a_group_moves_and_scales_its_outlines_and_strokes() {
    let layer = r##"{"id":"s","shapes":[{"group":[{"rect":[10,10]},{"stroke":"#000000","width":4}],"position":[100,0],"scale":2}]}"##;
    assert_eq!(
        rect_of(layer, "s", false),
        Rect::new(90.0, -10.0, 110.0, 10.0)
    );
    assert_eq!(
        rect_of(layer, "s", true),
        Rect::new(86.0, -14.0, 114.0, 14.0)
    );
}

#[test]
fn a_rectangle_or_composition_covers_its_box() {
    assert_eq!(
        rect_of(r#"{"id":"r","rect":"30x20"}"#, "r", false),
        Rect::new(0.0, 0.0, 30.0, 20.0)
    );
    assert_eq!(
        rect_of(r#"{"id":"c","comp":"card"}"#, "c", true),
        Rect::new(0.0, 0.0, 200.0, 100.0)
    );
}

#[test]
fn a_composition_s_layer_is_found_where_the_scene_writes_it() {
    let g = graph(r#"{"id":"c","comp":"card"}"#);
    let inner = Address::written(Some("card"), "inner".parse().unwrap());
    let missing = Address::written(Some("card"), "nothing".parse().unwrap());
    let absent = Address::layer(["nothing".parse().unwrap()]);
    let rects = content_rects(
        &g,
        &[
            (inner, 1.5, false),
            (missing, 0.0, false),
            (absent, 0.0, false),
        ],
    );
    assert_eq!(rects, [Some(Rect::new(0.0, 0.0, 30.0, 20.0)), None, None]);
}
