//! Unit tests for `src/render/text.rs`.

use motion_scene::ContentState;
use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::render::tests::{scene, session};

/// The leaves of text layer `layer` (its keys after the id) at `frame`,
/// drawn straight from the module.
fn text_draws(layer: &str, frame: f64) -> Vec<Node> {
    let session = session(&scene(&format!(r#"{{"id":"t",{layer}}}"#)));
    let state = session.frame(frame).unwrap();
    match &state.nodes[0].content {
        ContentState::Text { text, glyphs } => draws(text, glyphs),
        other => panic!("not text: {other:?}"),
    }
}

fn glyphs_of(node: &Node) -> &GlyphDraw {
    match node {
        Node::Glyphs(glyphs) => glyphs,
        other => panic!("not glyphs: {other:?}"),
    }
}

/// What each leaf draws: `fill` or the stroke's width.
fn styles(nodes: &[Node]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| match &glyphs_of(node).style {
            DrawStyle::Fill(_) => "fill".to_owned(),
            DrawStyle::Stroke(stroke) => format!("stroke {}", stroke.width),
        })
        .collect()
}

#[rstest]
#[case::small(4.5, 4.5 / 24.0)]
#[case::at_the_small_end(9.0, 9.0 / 24.0)]
#[case::half_way(22.5, 22.5 * (1.0 / 24.0 + 1.0 / 32.0) / 2.0)]
#[case::at_the_large_end(36.0, 36.0 / 32.0)]
#[case::large(64.0, 2.0)]
fn faux_bold_thickens_by_a_share_of_the_size(#[case] size: f64, #[case] width: f64) {
    assert!(
        (bold_width(size) - width).abs() < 1e-12,
        "{}",
        bold_width(size)
    );
}

#[test]
fn a_plain_run_is_one_fill() {
    let nodes = text_draws(
        r##""text":"Hi","font":"Inter Variable 20","fill":"#FF0000""##,
        0.0,
    );
    let [leaf] = nodes.as_slice() else {
        panic!("one leaf: {nodes:?}")
    };
    let run = glyphs_of(leaf);
    assert_eq!(run.glyphs.len(), 2);
    assert_eq!(run.size, 20.0);
    assert_eq!(run.glyph_transform, None);
    assert_eq!(
        run.paint,
        Paint::Solid(motion_raster::Color::rgba(1.0, 0.0, 0.0, 1.0))
    );
    // Point text starts at the first baseline.
    assert_eq!(run.glyphs[0].y, 0.0);
}

#[rstest]
#[case::fill_over("fill-over", ["stroke 4", "fill"])]
#[case::stroke_over("stroke-over", ["fill", "stroke 4"])]
fn the_paint_order_sets_which_is_drawn_last(#[case] order: &str, #[case] expected: [&str; 2]) {
    let nodes = text_draws(
        &format!(
            r##""text":"Hi","font":"Inter Variable 20","style":{{"stroke":"#000000","stroke-width":4,"paint":"{order}"}}"##
        ),
        0.0,
    );
    assert_eq!(styles(&nodes), expected);
}

#[rstest]
#[case::clear(r##""stroke":"#00000000","stroke-width":4"##)]
#[case::no_width(r##""stroke":"#000000","stroke-width":0"##)]
fn a_stroke_that_leaves_no_mark_is_not_drawn(#[case] style: &str) {
    let nodes = text_draws(
        &format!(r#""text":"Hi","font":"Inter Variable 20","style":{{{style}}}"#),
        0.0,
    );
    assert_eq!(styles(&nodes), ["fill"]);
}

#[test]
fn faux_bold_strokes_the_fill_and_widens_the_stroke() {
    let nodes = text_draws(
        r##""text":"Hi","font":"Inter Variable 36","style":{"faux-bold":true,"stroke":"#000000","stroke-width":2}"##,
        0.0,
    );
    let bold = 36.0 / 32.0;
    assert_eq!(
        styles(&nodes),
        [
            format!("stroke {}", 2.0 + bold),
            "fill".to_owned(),
            format!("stroke {bold}")
        ]
    );
    // The thickening stroke is the fill's colour.
    assert_eq!(glyphs_of(&nodes[1]).paint, glyphs_of(&nodes[2]).paint);
}

#[test]
fn a_translucent_faux_bold_fill_takes_its_alpha_once() {
    let nodes = text_draws(
        r##""text":"Hi","font":"Inter Variable 36","fill":"#FF000080","style":{"faux-bold":true}"##,
        0.0,
    );
    let [Node::Layer(layer)] = nodes.as_slice() else {
        panic!("one layer: {nodes:?}")
    };
    assert_eq!(layer.opacity, 128.0 / 255.0);
    assert!(layer.is_isolated());
    assert_eq!(
        styles(&layer.children),
        ["fill".to_owned(), format!("stroke {}", 36.0 / 32.0)]
    );
    for leaf in &layer.children {
        assert_eq!(
            glyphs_of(leaf).paint,
            Paint::Solid(motion_raster::Color::rgba(1.0, 0.0, 0.0, 1.0))
        );
    }
}

#[test]
fn faux_italic_slants_each_glyph_about_its_origin() {
    let nodes = text_draws(
        r#""text":"Hi","font":"Inter Variable 20","style":{"faux-italic":true}"#,
        0.0,
    );
    let slant = glyphs_of(&nodes[0]).glyph_transform.expect("a slant");
    // A point 10 px above the baseline leans right by tan 12° of that.
    let top = slant * motion_raster::kurbo::Point::new(0.0, -10.0);
    assert!((top.x - 10.0 * 12f64.to_radians().tan()).abs() < 1e-12);
    assert_eq!(top.y, -10.0);
    assert_eq!(
        slant * motion_raster::kurbo::Point::ORIGIN,
        motion_raster::kurbo::Point::ORIGIN
    );
}

/// Two letters, the second chosen by an animator that sets `offsets`.
fn chosen_second(style: &str, offsets: &str) -> Vec<Node> {
    text_draws(
        &format!(
            r#""text":"ab","font":"Inter Variable 20","style":{{{style}}},"animate":[{{"animator":[{{"range":"char","start":0.5}}],{offsets}}}]"#
        ),
        0.0,
    )
}

#[test]
fn glyphs_in_the_same_state_are_drawn_together() {
    let nodes = chosen_second("", r#""position":[0,10]"#);
    // a, then b moved by a layer of its own.
    let [Node::Glyphs(a), Node::Layer(b)] = nodes.as_slice() else {
        panic!("two draws: {nodes:?}")
    };
    assert_eq!(a.glyphs.len(), 1);
    assert_eq!(b.transform, Affine::translate((0.0, 10.0)));
    assert!(!b.is_isolated());
}

#[test]
fn a_fading_glyph_with_one_paint_folds_its_opacity() {
    let nodes = chosen_second("", r#""opacity":0.25"#);
    let Node::Glyphs(b) = &nodes[1] else {
        panic!("b drawn straight: {nodes:?}")
    };
    let Paint::Solid(color) = b.paint;
    assert_eq!(color.a, 0.25);
}

#[test]
fn a_fading_glyph_with_fill_and_stroke_fades_as_one_shape() {
    let nodes = chosen_second(
        r##""stroke":"#000000","stroke-width":2"##,
        r#""opacity":0.25"#,
    );
    // a's stroke and fill, then b in a buffer of its own.
    let [_, _, Node::Layer(b)] = nodes.as_slice() else {
        panic!("three draws: {nodes:?}")
    };
    assert_eq!(b.opacity, 0.25);
    assert!(b.is_isolated());
    assert_eq!(styles(&b.children), ["stroke 2", "fill"]);
}

#[test]
fn an_invisible_glyph_is_not_drawn() {
    let nodes = chosen_second("", r#""opacity":0"#);
    let counts: Vec<usize> = nodes.iter().map(|n| glyphs_of(n).glyphs.len()).collect();
    assert_eq!(counts, [1]);
}

#[test]
fn a_glyph_takes_its_animated_colours_and_stroke_width() {
    let nodes = chosen_second(
        r##""stroke":"#000000","stroke-width":2"##,
        r##""fill":"#00FF00","stroke":"#0000FF","stroke-width":3"##,
    );
    // b is not moved, so its stroke and fill follow a's, drawn straight.
    assert_eq!(styles(&nodes), ["stroke 2", "fill", "stroke 3", "fill"]);
    let paint = |i: usize| glyphs_of(&nodes[i]).paint.clone();
    assert_eq!(
        paint(2),
        Paint::Solid(motion_raster::Color::rgba(0.0, 0.0, 1.0, 1.0))
    );
    assert_eq!(
        paint(3),
        Paint::Solid(motion_raster::Color::rgba(0.0, 1.0, 0.0, 1.0))
    );
}
