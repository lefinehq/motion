//! Unit tests for `src/text.rs`: content handed to the typesetter.

use super::*;
use motion_format::parse_document;
use motion_format::text::Align;
use motion_text::{FontSet, Justification};

/// The text content of the one layer of a scene whose layer keys are `keys`.
fn content(keys: &str) -> TextContent {
    let json = format!(r#"{{"motion":1,"dur":"1s","layers":[{{"id":"t",{keys}}}]}}"#);
    match &parse_document(&json).unwrap().scene().layers[0].content {
        motion_format::Content::Text(text) => (**text).clone(),
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
            motion_text::Leading::Px(30.0),
            4.0,
            motion_text::Caps::All,
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
    for (_, align) in motion_format::text::ALIGNS {
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
    assert_eq!(error.kind, motion_format::Kind::UnknownName);
    assert!(error.message.contains("Lato-Black"), "{}", error.message);
    assert_eq!(error.path.as_deref(), Some("font"));
    // A face a run asks for is reported at that run.
    let error = set(
        &content(
            r#""text":"ab","runs":[{"range":[0,1],"caps":"all"},{"range":[1,2],"font":"Nope 10"}]"#,
        ),
        &mut typesetter,
    )
    .unwrap_err();
    assert_eq!(error.path.as_deref(), Some("runs[1].font"));
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
