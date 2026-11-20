//! Unit tests for `src/validate/text.rs`: reading a text layer's style.

use crate::text::{Align, Caps, Leading, PaintOrder, TextContent};
use crate::{Content, Kind, parse_document};

fn text(keys: &str) -> TextContent {
    let json = format!(r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"t",{keys}}}]}}"#);
    match &parse_document(&json).unwrap().scene().root.layers[0].content {
        Content::Text(text) => (**text).clone(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn the_base_style_comes_from_font_fill_and_style() {
    let t = text(
        r##""text":"Hi","font":"Lato-Black 55","fill":[0.5,0.5,0.5],"style":{"stroke":"#000000","stroke-width":2,"paint":"stroke-over","tracking":25,"leading":"auto","baseline":-3,"caps":"all","faux-bold":true,"faux-italic":true,"italic":true,"align":"right","indent-first":4,"indent-start":5,"indent-end":6,"space-before":7,"space-after":8,"auto-leading":1.5}"##,
    );
    let s = t.first_style();
    assert_eq!((s.font.family.as_str(), s.font.size), ("Lato-Black", 55.0));
    assert_eq!(s.fill, [0.5, 0.5, 0.5, 1.0]);
    assert_eq!(
        (s.stroke, s.stroke_width, s.paint),
        ([0.0, 0.0, 0.0, 1.0], 2.0, PaintOrder::StrokeOver)
    );
    assert_eq!(
        (s.tracking, s.leading, s.baseline, s.caps),
        (25.0, Leading::Auto, -3.0, Caps::All)
    );
    assert!(s.faux_bold && s.faux_italic && s.italic);
    let p = t.paragraphs[0];
    assert_eq!(p.align, Align::Right);
    assert_eq!(
        [
            p.indent_first,
            p.indent_start,
            p.indent_end,
            p.space_before,
            p.space_after,
            p.auto_leading
        ],
        [4.0, 5.0, 6.0, 7.0, 8.0, 1.5]
    );
    assert_eq!(t.frame, None);
}

#[test]
fn runs_split_the_text_and_later_runs_win() {
    let t = text(
        r##""text":"abcdef","runs":[{"range":[1,4],"fill":"#FF0000"},{"range":[3,5],"tracking":10,"leading":20}]"##,
    );
    let spans: Vec<_> = t.runs.iter().map(|r| r.range.clone()).collect();
    assert_eq!(spans, [0..1, 1..3, 3..5, 5..6]);
    assert_eq!(t.runs[1].style.fill, [1.0, 0.0, 0.0, 1.0]);
    // The later run starts from the base style, not the earlier run's.
    assert_eq!(t.runs[2].style.fill, [1.0; 4]);
    assert_eq!(
        (t.runs[2].style.tracking, t.runs[2].style.leading),
        (10.0, Leading::Px(20.0))
    );
}

#[test]
fn runs_count_characters_not_bytes() {
    let t = text(r##""text":"éé","runs":[{"range":[1,2],"caps":"small"}]"##);
    assert_eq!(t.runs[1].range, 1..2);
}

#[test]
fn each_paragraph_takes_its_own_style_and_the_rest_the_base() {
    let t = text(r#""text":"a\nb\nc","style":{"align":"center"},"paragraphs":[{"align":"left"}]"#);
    let aligns: Vec<Align> = t.paragraphs.iter().map(|p| p.align).collect();
    assert_eq!(aligns, [Align::Left, Align::Center, Align::Center]);
}

#[test]
fn empty_text_has_one_base_run_and_a_box_makes_box_text() {
    let t = text(r#""text":"","box":[300,120]"#);
    assert_eq!(t.runs.len(), 1);
    assert_eq!(t.runs[0].range, 0..0);
    assert_eq!(t.frame, Some(kurbo::Rect::new(0.0, 0.0, 300.0, 120.0)));
}

#[test]
fn a_box_may_start_away_from_the_layers_origin() {
    let t = text(r#""text":"","box":[-150,-60.5,300,120]"#);
    assert_eq!(t.frame, Some(kurbo::Rect::new(-150.0, -60.5, 150.0, 59.5)));
}

#[test]
fn fit_reads_a_size_for_point_text_and_none_for_box_text() {
    use crate::text::TextFit;
    let fit = |keys: &str| text(&format!(r#""text":"Hi",{keys}"#)).fit;
    assert_eq!(fit(r#""font":"Inter 40""#), None);
    assert_eq!(
        fit(r#""fit":"shrink 1240x310""#),
        Some(TextFit::Shrink {
            width: 1240.0,
            height: 310.0
        })
    );
    assert_eq!(
        fit(r#""fit":"wrap 600.5x300""#),
        Some(TextFit::Wrap {
            width: 600.5,
            height: 300.0
        })
    );
    assert_eq!(
        fit(r#""box":[300,120],"fit":"shrink""#),
        Some(TextFit::ShrinkToBox)
    );
}

#[test]
fn a_fit_that_does_not_suit_its_text_is_an_error_at_fit() {
    let error = |keys: &str| {
        let json =
            format!(r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"t","text":"Hi",{keys}}}]}}"#);
        let e = parse_document(&json).unwrap_err();
        (e.kind, e.path_text(), e.expected.map(|e| e.to_string()))
    };
    let point = Some(
        "`shrink <width>x<height>` or `wrap <width>x<height>`, e.g. `shrink 600x300`".to_owned(),
    );
    let boxed = Some("`shrink`, which shrinks box text until its lines fit its box".to_owned());
    let at = Some("t.fit".to_owned());
    assert_eq!(
        error(r#""fit":"shrink""#),
        (Kind::Missing, at.clone(), point)
    );
    for fit in ["shrink 300x100", "wrap 300x100"] {
        assert_eq!(
            error(&format!(r#""box":[300,120],"fit":"{fit}""#)),
            (Kind::Conflict, at.clone(), boxed.clone()),
            "{fit}"
        );
    }
    let (kind, path, expected) = error(r#""fit":"grow 300x100""#);
    assert_eq!((kind, path), (Kind::Syntax, at.clone()));
    assert!(expected.is_some_and(|e| e.contains("`wrap <width>x<height>`")));
    assert_eq!(error(r#""fit":3"#).1, at);
}

#[test]
fn fit_only_applies_to_text_layers() {
    let json =
        r#"{"nakkash":1,"dur":"1s","layers":[{"id":"r","rect":"10x10","fit":"shrink 5x5"}]}"#;
    let e = parse_document(json).unwrap_err();
    assert_eq!(
        (e.kind, e.path_text().as_deref()),
        (Kind::Conflict, Some("r.fit"))
    );
}

#[test]
fn a_fit_scales_every_length_that_goes_with_the_size() {
    let t = text(
        r##""text":"Hi\nthere","font":"Inter 40","style":{"stroke":"#000000","stroke-width":2,"tracking":25,"leading":50,"baseline":-3,"indent-first":4,"indent-start":5,"indent-end":6,"space-before":7,"space-after":8,"auto-leading":1.5},"runs":[{"range":[0,1],"font":"Inter 80"}]"##,
    );
    let half = t.scaled(0.5);
    let s = &half.runs[0].style;
    assert_eq!(
        (
            s.font.size,
            s.stroke_width,
            s.leading,
            s.baseline,
            s.tracking
        ),
        (40.0, 1.0, Leading::Px(25.0), -1.5, 25.0)
    );
    assert_eq!(half.base.font.size, 20.0);
    let p = half.paragraphs[1];
    assert_eq!(
        [
            p.indent_first,
            p.indent_start,
            p.indent_end,
            p.space_before,
            p.space_after,
            p.auto_leading
        ],
        [2.0, 2.5, 3.0, 3.5, 4.0, 1.5]
    );
    assert_eq!((half.text.as_str(), half.frame), ("Hi\nthere", t.frame));
}

/// A text layer's words keyed in `anim.text` ([D92]): held keys of strings
/// in time order, read into the content, not into the layer's tracks.
#[test]
fn keyed_words_are_the_contents_keys() {
    let t = text(
        r##""text":"10","font":"Inter 40","anim":{"text":[[0,"3"],["1s","2",  "hold"],{"t":"2s","v":"1\nGo"}]}"##,
    );
    let keys: Vec<(f64, &str)> = t
        .keys
        .iter()
        .map(|k| (k.time.to_frames(30.0), k.text.as_str()))
        .collect();
    assert_eq!(keys, [(0.0, "3"), (30.0, "2"), (60.0, "1\nGo")]);
    assert_eq!(t.text, "10");
    let json = r#"{"nakkash":1,"dur":"1s","layers":[{"id":"t","text":"a","anim":{"text":[[0,"b"]],"opacity":"0 -> 1 @0-5f"}}]}"#;
    let scene = parse_document(json).unwrap();
    assert_eq!(scene.scene().root.layers[0].anim.len(), 1, "opacity only");
}

#[test]
fn keyed_words_that_break_a_rule_are_errors_at_the_key() {
    let error = |layer: &str| {
        let json = format!(r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"t",{layer}}}]}}"#);
        let e = parse_document(&json).unwrap_err();
        (e.kind, e.path_text().unwrap_or_default())
    };
    let keyed = |track: &str| error(&format!(r#""text":"a","anim":{{"text":{track}}}"#));
    assert_eq!(
        keyed(r#""a -> b @0-5f""#),
        (Kind::Invalid, "t.anim.text".into())
    );
    assert_eq!(keyed("[]"), (Kind::Missing, "t.anim.text".into()));
    assert_eq!(
        keyed(r#"[[0, 3]]"#),
        (Kind::Invalid, "t.anim.text[0][1]".into())
    );
    assert_eq!(
        keyed(r#"[[0, "a", "linear"]]"#),
        (Kind::Invalid, "t.anim.text[0][2]".into())
    );
    assert_eq!(
        keyed(r#"[{"t":0,"v":"a","path":[[0,0],[0,0]]}]"#),
        (Kind::Invalid, "t.anim.text[0].path".into())
    );
    assert_eq!(
        keyed(r#"[["1s","a"],[0,"b"]]"#),
        (Kind::Invalid, "t.anim.text[1]".into())
    );
    assert_eq!(
        error(r#""rect":"10x10","anim":{"text":[[0,"a"]]}"#),
        (Kind::Conflict, "t.anim.text".into())
    );
}

/// A key's words take the layer's base style and first paragraph's: the
/// layer's runs and paragraph styles count its own words.
#[test]
fn a_keys_words_take_the_layers_base_style() {
    let t = text(
        r##""text":"Hi\nthere","font":"Inter 40","fill":"#FF0000","style":{"align":"center"},"runs":[{"range":[0,1],"fill":"#00FF00"}],"paragraphs":[{},{"align":"right"}]"##,
    );
    let keyed = t.with_words("a\nb\nc");
    assert_eq!(keyed.text, "a\nb\nc");
    assert_eq!(keyed.runs.len(), 1);
    assert_eq!(keyed.runs[0].range, 0..5);
    assert_eq!(keyed.runs[0].style.fill, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(keyed.paragraphs.len(), 3);
    assert!(keyed.paragraphs.iter().all(|p| p.align == Align::Center));
    assert!(keyed.keys.is_empty());
}
