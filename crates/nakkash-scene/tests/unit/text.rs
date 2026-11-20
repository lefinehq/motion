//! Unit tests for `src/text.rs`: content handed to the typesetter.

use super::*;
use nakkash_format::parse_document;
use nakkash_format::text::Align;
use nakkash_text::{FontSet, Justification};

/// The text content of the one layer of a scene whose layer keys are `keys`.
fn content(keys: &str) -> TextContent {
    let json = format!(r#"{{"nakkash":1,"dur":"1s","layers":[{{"id":"t",{keys}}}]}}"#);
    match &parse_document(&json).unwrap().scene().root.layers[0].content {
        nakkash_format::Content::Text(text) => (**text).clone(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn a_postscript_name_asks_for_that_face_and_a_family_for_its_weight() {
    let fonts = FontSet::builtin();
    let by_name = styled(
        &content(r#""text":"a","font":"InterVariable-Bold 20""#),
        &fonts,
    );
    assert_eq!(
        by_name.runs[0].style.font,
        FontQuery::PostScript("InterVariable-Bold".into())
    );
    let by_family = styled(
        &content(r#""text":"a","font":"Inter Variable 20 700","style":{"italic":true}"#),
        &fonts,
    );
    assert_eq!(
        by_family.runs[0].style.font,
        FontQuery::Family {
            family: "Inter Variable".into(),
            weight: 700.0,
            slant: Slant::Italic,
        }
    );
}

#[test]
fn runs_count_characters_and_the_typesetter_bytes() {
    let text = content(r##""text":"é€a","runs":[{"range":[1,2],"fill":"#FF0000"}]"##);
    let styled = styled(&text, &FontSet::builtin());
    let ranges: Vec<_> = styled.runs.iter().map(|r| r.range.clone()).collect();
    // `é` is two bytes and `€` three.
    assert_eq!(ranges, [0..2, 2..5, 5..6]);
}

#[test]
fn an_empty_text_lays_out_as_nothing() {
    let text = content(r#""text":"","font":"InterVariable 20""#);
    assert_eq!(text.runs.len(), 1, "the format keeps a base run");
    let styled = styled(&text, &FontSet::builtin());
    assert!(styled.runs.is_empty());
    let layout = nakkash_text::Typesetter::new(FontSet::builtin())
        .layout(&styled)
        .expect("an empty text lays out");
    assert!(layout.lines.is_empty());
}

#[test]
fn style_paragraphs_and_box_reach_the_typesetter() {
    let text = content(
        r#""text":"a\nb","box":[300,200],"style":{"tracking":50,"caps":"all","leading":30,"baseline":4,"faux-bold":true},"paragraphs":[{"align":"center"},{"align":"justify-all","indent-first":10}]"#,
    );
    assert_eq!(text.paragraphs[0].align, Align::Center);
    let styled = styled(&text, &FontSet::builtin());
    let style = &styled.runs[0].style;
    assert_eq!(
        (
            style.tracking,
            style.leading,
            style.baseline_shift,
            style.caps,
            style.faux_bold
        ),
        (
            50.0,
            nakkash_text::Leading::Px(30.0),
            4.0,
            nakkash_text::Caps::All,
            true
        )
    );
    assert_eq!(
        (
            styled.paragraphs[0].justification,
            styled.paragraphs[1].justification
        ),
        (Justification::Center, Justification::JustifyAll)
    );
    assert_eq!(styled.paragraphs[1].first_line_indent, 10.0);
    assert_eq!(
        styled.frame,
        TextFrame::Box {
            left: 0.0,
            top: 0.0,
            width: 300.0,
            height: 200.0
        }
    );
}

#[test]
fn every_alignment_has_a_justification() {
    for (_, align) in nakkash_format::text::ALIGNS {
        justification(align);
    }
}

#[test]
fn a_missing_font_is_an_error_not_a_fallback() {
    let mut typesetter = Typesetter::new(FontSet::builtin());
    let error = set(
        &content(r#""text":"a","font":"Lato-Black 20""#),
        &mut typesetter,
    )
    .unwrap_err();
    assert_eq!(error.kind, nakkash_format::Kind::UnknownName);
    assert!(error.message.contains("Lato-Black"), "{}", error.message);
    assert_eq!(error.path_text().as_deref(), Some("font"));
    // A face a run asks for is reported at that run.
    let error = set(
        &content(
            r#""text":"ab","runs":[{"range":[0,1],"caps":"all"},{"range":[1,2],"font":"Nope 10"}]"#,
        ),
        &mut typesetter,
    )
    .unwrap_err();
    assert_eq!(error.path_text().as_deref(), Some("runs[1].font"));
}

#[test]
fn a_set_node_keeps_its_runs_styles() {
    let mut typesetter = Typesetter::new(FontSet::builtin());
    let node = set(
        &content(r##""text":"ab","runs":[{"range":[1,2],"fill":"#FF0000"}]"##),
        &mut typesetter,
    )
    .unwrap();
    let last = node.layout.runs.len() - 1;
    assert_eq!(node.style_of(last).fill, [1.0, 0.0, 0.0, 1.0]);
    assert!(node.layout.bounds().width() > 0.0);
}

#[test]
fn a_fitted_text_scales_the_content_it_keeps_and_as_written_leaves_the_fit_out() {
    let keys = r##""text":"Extraordinarily","font":"Inter Variable 40","style":{"stroke":"#000000","stroke-width":4},"fit":"shrink 150x100""##;
    let fitted = content(keys);
    assert_eq!(
        styled(&fitted, &FontSet::builtin()).fit,
        Some(Fit::Shrink {
            width: 150.0,
            height: 100.0
        })
    );
    let mut typesetter = Typesetter::new(FontSet::builtin());
    let node = set(&fitted, &mut typesetter).unwrap();
    let s = node.layout.scale;
    assert!(s < 1.0 && node.layout.size().width <= 150.0);
    let style = node.content.first_style();
    assert_eq!((style.font.size, style.stroke_width), (40.0 * s, 4.0 * s));
    let written = set_as_written(&fitted, &mut typesetter).unwrap();
    assert_eq!(written.layout.scale, 1.0);
    assert!(written.layout.size().width > 150.0);
    assert_eq!(written.content.first_style().font.size, 40.0);
}

/// Keyed words ([D92]) are set each on its own and shown from their key's
/// frame of the layer's own time to the next, the first before any; keys
/// of the same words share one layout.
#[test]
fn keyed_words_are_set_on_their_own_and_shown_from_their_key() {
    let keyed = content(
        r#""text":"10","font":"Inter Variable 40","anim":{"text":[[0,"3"],["1s","22"],["2s","3"]]}"#,
    );
    let mut typesetter = Typesetter::new(FontSet::builtin());
    let (own, keys) = set_keyed(&keyed, &mut typesetter, 30.0).unwrap();
    assert_eq!(own.content.text, "10");
    let frames: Vec<f64> = keys.iter().map(|k| k.frame).collect();
    assert_eq!(frames, [0.0, 30.0, 60.0]);
    assert!(
        Arc::ptr_eq(&keys[0].text, &keys[2].text),
        "one layout for one text"
    );
    let at = |local: f64| shown(&own, &keys, local).content.text.as_str();
    assert_eq!(at(-5.0), "3", "the first key's before it");
    assert_eq!(at(29.0), "3");
    assert_eq!(
        at(30.0 - 1e-4),
        "22",
        "a key a hair after the frame counts as reached"
    );
    assert_eq!(at(59.0), "22");
    assert_eq!(at(1000.0), "3");
    assert!(keys[1].text.layout.bounds().width() > keys[0].text.layout.bounds().width());
    // Without keys, the layer shows its own words.
    let still = content(r#""text":"10","font":"Inter Variable 40""#);
    let (own, keys) = set_keyed(&still, &mut typesetter, 30.0).unwrap();
    assert!(keys.is_empty());
    assert_eq!(shown(&own, &keys, 12.0).content.text, "10");
}

/// A fit sets the layer's own words and every key's at one scale, the
/// smallest any of them takes, so the size does not jump from key to key.
#[test]
fn a_fit_sets_every_keyed_text_at_one_scale() {
    let keyed = content(
        r#""text":"Hi","font":"Inter Variable 40","fit":"shrink 150x100","anim":{"text":[[0,"Hi"],[10,"Extraordinarily"]]}"#,
    );
    let mut typesetter = Typesetter::new(FontSet::builtin());
    let (own, keys) = set_keyed(&keyed, &mut typesetter, 30.0).unwrap();
    let alone = set(
        &content(r#""text":"Extraordinarily","font":"Inter Variable 40","fit":"shrink 150x100""#),
        &mut typesetter,
    )
    .unwrap();
    let s = alone.layout.scale;
    assert!(s < 1.0);
    for node in [&own, &keys[0].text, &keys[1].text] {
        assert_eq!(node.layout.scale, s, "{}", node.content.text);
        assert_eq!(node.content.first_style().font.size, 40.0 * s);
    }
    assert_eq!(keys[1].text.layout, alone.layout);
    // A text that fits keeps its place: shrink keeps the middle of its lines.
    let small = set(
        &content(r#""text":"Hi","font":"Inter Variable 40""#),
        &mut typesetter,
    )
    .unwrap();
    let (a, b) = (
        small.layout.bounds().center().y,
        own.layout.bounds().center().y,
    );
    assert!((a - b).abs() < 1e-6, "{a} {b}");
}

/// A key whose words the font cannot set is an error at that key.
#[test]
fn keyed_words_a_font_cannot_set_are_an_error_at_the_key() {
    let keyed =
        content(r#""text":"a","font":"Inter Variable 40","anim":{"text":[[0,"b"],[5,"͸"]]}"#);
    let mut typesetter = Typesetter::new(FontSet::builtin());
    let error = set_keyed(&keyed, &mut typesetter, 30.0).unwrap_err();
    assert!(
        error
            .path_text()
            .is_some_and(|p| p.starts_with("anim.text[1]")),
        "{error:?}"
    );
}
