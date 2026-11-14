//! Unit tests for `src/animate.rs`: parts, selector shapes and modes, what
//! animators do to glyphs, and the bounds of animated text.

use super::*;
use crate::{ContentState, FrameState, NodeState, compile, evaluate};
use motion_format::parse_document;
use motion_text::{FontSet, Typesetter};
use rstest::rstest;

/// The frame `frame` of a 10 fps scene whose one text layer `t` has `keys`.
fn frame(keys: &str, frame: f64) -> FrameState {
    let json = format!(r#"{{"motion":1,"dur":"2s","fps":10,"layers":[{{"id":"t",{keys}}}]}}"#);
    let doc = parse_document(&json).unwrap();
    let graph = compile(doc.scene(), &mut Typesetter::new(FontSet::builtin())).unwrap();
    evaluate(&graph, frame)
}

fn text_of(node: &NodeState) -> (&TextNode, &[GlyphState]) {
    match &node.content {
        ContentState::Text { text, glyphs } => (text, glyphs),
        other => panic!("not text: {other:?}"),
    }
}

/// Each glyph's opacity, rounded to 3 places, at `frame` of `keys`.
fn opacities(keys: &str, at: f64) -> Vec<f64> {
    let state = frame(keys, at);
    let (_, glyphs) = text_of(&state.nodes[0]);
    glyphs
        .iter()
        .map(|g| (g.opacity * 1000.0).round() / 1000.0)
        .collect()
}

#[test]
fn every_glyph_knows_its_character_word_and_line() {
    let state = frame(r#""text":"ab c\nd""#, 0.0);
    let (text, _) = text_of(&state.nodes[0]);
    let index: Vec<[Option<usize>; 4]> = text.parts.glyphs.iter().map(|g| g.index).collect();
    assert_eq!(
        index,
        [
            [Some(0), Some(0), Some(0), Some(0)],
            [Some(1), Some(1), Some(0), Some(0)],
            [Some(2), None, None, Some(0)],
            [Some(3), Some(2), Some(1), Some(0)],
            [Some(4), Some(3), Some(2), Some(1)],
        ]
    );
    assert_eq!(text.parts.counts, [5, 4, 3, 2]);
}

#[test]
fn without_animators_no_glyph_state_is_kept() {
    let state = frame(r#""text":"ab""#, 0.0);
    assert!(text_of(&state.nodes[0]).1.is_empty());
}

/// Four characters faded by a range selector of `selector` keys.
fn fade(selector: &str) -> String {
    format!(
        r#""text":"abcd","animate":[{{"animator":[{{"range":"char"{selector}}}],"opacity":0}}]"#
    )
}

#[rstest]
// The whole range chooses every character fully.
#[case::all(r#""#, [0.0, 0.0, 0.0, 0.0])]
// Half way: two characters out, two in.
#[case::offset_half(r#","offset":0.5"#, [1.0, 1.0, 0.0, 0.0])]
// A range ending in the middle of the second character covers half of it.
#[case::partial(r#","end":0.375"#, [0.0, 0.5, 1.0, 1.0])]
// Without smoothing, the middle decides.
#[case::hard_edge(r#","end":0.4,"smooth":0"#, [0.0, 0.0, 1.0, 1.0])]
#[case::ramp_up(r#","shape":"ramp-up""#, [0.875, 0.625, 0.375, 0.125])]
#[case::ramp_down(r#","shape":"ramp-down""#, [0.125, 0.375, 0.625, 0.875])]
#[case::triangle(r#","shape":"triangle""#, [0.75, 0.25, 0.25, 0.75])]
#[case::round(r#","shape":"round""#, [0.339, 0.032, 0.032, 0.339])]
#[case::smooth(r#","shape":"smooth""#, [0.854, 0.146, 0.146, 0.854])]
// Half the amount fades half way.
#[case::half_amount(r#","amount":0.5"#, [0.5, 0.5, 0.5, 0.5])]
// After the range, a ramp up stays chosen.
#[case::ramp_up_after(r#","shape":"ramp-up","end":0.5"#, [0.75, 0.25, 0.0, 0.0])]
fn range_selectors_choose_by_their_shape(#[case] selector: &str, #[case] expected: [f64; 4]) {
    assert_eq!(opacities(&fade(selector), 0.0), expected);
}

#[rstest]
#[case::add(r#""add""#, [0.0, 0.0, 0.0, 0.0])]
#[case::subtract(r#""subtract""#, [0.0, 0.0, 1.0, 1.0])]
#[case::intersect(r#""intersect""#, [1.0, 1.0, 0.0, 0.0])]
#[case::min(r#""min""#, [1.0, 1.0, 0.0, 0.0])]
#[case::max(r#""max""#, [0.0, 0.0, 0.0, 0.0])]
#[case::difference(r#""difference""#, [0.0, 0.0, 1.0, 1.0])]
fn a_second_selector_combines_by_its_mode(#[case] mode: &str, #[case] expected: [f64; 4]) {
    // The first chooses every character; the second the last two.
    let keys = format!(
        r#""text":"abcd","animate":[{{"animator":[{{"range":"char"}},{{"range":"char","start":0.5,"mode":{mode}}}],"opacity":0}}]"#
    );
    assert_eq!(opacities(&keys, 0.0), expected);
}

#[test]
fn a_subtracting_first_selector_takes_from_everything() {
    let keys = r#""text":"abcd","animate":[{"animator":[{"range":"char","end":0.5,"mode":"subtract"}],"opacity":0}]"#;
    assert_eq!(opacities(keys, 0.0), [1.0, 1.0, 0.0, 0.0]);
}

#[test]
fn words_and_lines_are_chosen_whole_and_spaces_by_neither() {
    let by_word =
        r#""text":"ab cd","animate":[{"animator":[{"range":"word","end":0.5}],"opacity":0}]"#;
    assert_eq!(opacities(by_word, 0.0), [0.0, 0.0, 1.0, 1.0, 1.0]);
    let by_line =
        r#""text":"a\nb","animate":[{"animator":[{"range":"line","start":0.5}],"opacity":0}]"#;
    assert_eq!(opacities(by_line, 0.0), [1.0, 0.0]);
}

#[test]
fn the_title_lines_rise_one_after_another() {
    // The first sample's title: lines start 75 px low and clear, and an
    // offset from 0 to 1 over 2 s lets them in, the first line first.
    let keys = r#""text":"Keep\nyour life\nsimple","style":{"align":"center"},"animate":[{"animator":[{"range":"line","anim":{"offset":[{"t":0,"v":0},{"t":20,"v":1}]}}],"position":[0,75],"opacity":0}]"#;
    let rise = |at: f64| {
        let state = frame(keys, at);
        let (text, glyphs) = text_of(&state.nodes[0]);
        // The first glyph of each line.
        let mut firsts = Vec::new();
        let mut at_glyph = 0;
        for run in &text.layout.runs {
            if firsts.len() == run.line {
                let g = &glyphs[at_glyph];
                firsts.push((
                    (g.transform.translation().y * 1e6).round() / 1e6,
                    (g.opacity * 100.0).round() / 100.0,
                ));
            }
            at_glyph += run.glyphs.len();
        }
        firsts
    };
    assert_eq!(rise(0.0), [(75.0, 0.0), (75.0, 0.0), (75.0, 0.0)]);
    // A sixth of the way, half the first line is out of the range.
    assert_eq!(rise(10.0 / 3.0), [(37.5, 0.5), (75.0, 0.0), (75.0, 0.0)]);
    assert_eq!(rise(10.0), [(0.0, 1.0), (37.5, 0.5), (75.0, 0.0)]);
    assert_eq!(rise(20.0), [(0.0, 1.0), (0.0, 1.0), (0.0, 1.0)]);
}

#[test]
fn transforms_turn_and_scale_about_the_middle_of_the_baseline() {
    let keys = r#""text":"a","animate":[{"animator":[{"range":"char"}],"rotate":90,"scale":2,"position":[3,4]}]"#;
    let state = frame(keys, 0.0);
    let (text, glyphs) = text_of(&state.nodes[0]);
    let g = &text.layout.runs[0].glyphs[0];
    let pivot = kurbo::Point::new(g.x + g.advance / 2.0, g.y);
    let moved = glyphs[0].transform * pivot;
    assert!((moved.x - pivot.x - 3.0).abs() < 1e-9 && (moved.y - pivot.y - 4.0).abs() < 1e-9);
    // A point one px right of the pivot lands two px below it.
    let right = glyphs[0].transform * (pivot + Vec2::new(1.0, 0.0));
    assert!(
        (right - moved - Vec2::new(0.0, 2.0)).hypot() < 1e-9,
        "{right:?}"
    );
}

#[test]
fn colours_and_stroke_width_blend_toward_the_animator() {
    let keys = r##""text":"a","fill":"#000000","style":{"stroke":"#000000","stroke-width":2},"animate":[{"animator":[{"range":"char","amount":0.5}],"fill":"#FFFFFF","stroke":"#FF0000","stroke-width":4}]"##;
    let state = frame(keys, 0.0);
    let (_, glyphs) = text_of(&state.nodes[0]);
    assert_eq!(glyphs[0].fill, [0.5, 0.5, 0.5, 1.0]);
    assert_eq!(glyphs[0].stroke, [0.5, 0.0, 0.0, 1.0]);
    assert_eq!(glyphs[0].stroke_width, 3.0);
}

#[test]
fn tracking_pushes_the_glyphs_after_it_and_keeps_the_alignment() {
    let shift = |align: &str| {
        let keys = format!(
            r#""text":"abc","font":"Inter Variable 100","style":{{"align":"{align}"}},"animate":[{{"animator":[{{"range":"char"}}],"tracking":100}}]"#
        );
        let state = frame(&keys, 0.0);
        let (_, glyphs) = text_of(&state.nodes[0]);
        glyphs
            .iter()
            .map(|g| g.transform.translation().x)
            .collect::<Vec<_>>()
    };
    // 100 thousandths of a 100 px em is 10 px after each of three glyphs.
    assert_eq!(shift("left"), [0.0, 10.0, 20.0]);
    assert_eq!(shift("center"), [-15.0, -5.0, 5.0]);
    assert_eq!(shift("right"), [-30.0, -20.0, -10.0]);
}

#[test]
fn hidden_animators_and_unwritten_properties_do_nothing() {
    let keys = r##""text":"a","fill":"#000000","animate":[{"animator":[{"range":"char"}],"opacity":0,"hidden":true},{"animator":[{"range":"char"}],"rotate":10}]"##;
    let state = frame(keys, 0.0);
    let (_, glyphs) = text_of(&state.nodes[0]);
    assert_eq!(
        (glyphs[0].opacity, glyphs[0].fill),
        (1.0, [0.0, 0.0, 0.0, 1.0])
    );
}

#[test]
fn bounds_cover_the_visible_glyphs_where_they_are() {
    let keys = r#""text":"ab","animate":[{"animator":[{"range":"char","start":0.5}],"position":[0,100],"opacity":0}]"#;
    let state = frame(keys, 0.0);
    let (text, _) = text_of(&state.nodes[0]);
    let laid_out = text.layout.bounds();
    let shown = state.nodes[0].local_bounds;
    // Only `a` shows, where it was laid out.
    assert_eq!(
        (shown.x0, shown.y0, shown.y1),
        (laid_out.x0, laid_out.y0, laid_out.y1)
    );
    assert!(shown.x1 < laid_out.x1);
}

mod any_selector {
    use super::*;
    use motion_format::item::SELECTORS;
    use proptest::prelude::*;

    /// A range selector with `values` and the setting choices `settings`.
    fn selector(values: [f64; 5], settings: [usize; 3]) -> ItemState {
        ItemState {
            kind: &SELECTORS[0],
            id: None,
            values: values
                .iter()
                .map(|&v| motion_format::Value::Number(v))
                .collect(),
            settings: settings.to_vec(),
            items: Vec::new(),
        }
    }

    proptest! {
        #[test]
        fn chooses_between_minus_one_and_one(
            start in -2.0..2.0f64,
            end in -2.0..2.0f64,
            offset in -1.0..1.0f64,
            amount in -1.0..1.0f64,
            smooth in 0.0..1.0f64,
            shape in 0..6usize,
            mode in 0..6usize,
            by in 0..4usize,
            n in 1..40usize,
            i in 0..40usize,
        ) {
            let s = selector([start, end, offset, amount, smooth], [by, shape, mode]);
            let glyph = GlyphParts { index: [Some(i % n); 4] };
            let v = chosen(&[s.clone(), s], &glyph, &[n; 4]);
            prop_assert!((-1.0..=1.0).contains(&v), "{v}");
        }
    }
}
