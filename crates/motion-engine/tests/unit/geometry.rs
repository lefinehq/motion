//! Unit tests for `src/geometry.rs`: a scene's text held against layouts
//! written from itself, moved, and broken elsewhere.

use motion_import::{ComposedLine, ComposedSegment};
use pretty_assertions::assert_eq;

use super::*;

/// A scene with a two-paragraph centred title at the top level and a label
/// inside a composition.
const SCENE: &str = r##"{"motion":1,"size":"400x200","fps":10,"dur":"1s",
"comps":{"card":{"size":"200x100","dur":"1s","layers":[{"id":"label","text":"Label","font":"InterVariable 20"}]}},
"layers":[{"id":"title","text":"Keep\nit simple","font":"InterVariable 40","style":{"align":"center"},"at":"200 80"},
{"id":"card","comp":"card"},{"id":"box","rect":"10x10","fill":"#FFFFFF"}]}"##;

/// A session on `SCENE`.
fn session() -> Session {
    SCENE.parse().expect("a valid scene")
}

/// The lines the scene lays `layer` out in, written as a project stores
/// them: a line at its baseline, one segment from its start, the paragraph
/// end counted.
fn stored(session: &Session, comp: Option<&str>, layer: &str) -> StoredText {
    let text = StoredText {
        comp: comp.map(str::to_owned),
        layer: layer.into(),
        lines: Vec::new(),
    };
    let layout = layout_of(session.graph().unwrap(), &text).unwrap();
    let lines = layout
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let ends = layout
                .lines
                .get(i + 1)
                .is_none_or(|next| next.paragraph != line.paragraph);
            ComposedLine {
                origin: [0.0, line.baseline],
                baseline: line.baseline,
                segments: vec![ComposedSegment {
                    characters: line.characters + usize::from(ends),
                    origin: [line.x, 0.0],
                    advances: line.advances.clone(),
                    width: line.width,
                    glyphs: Vec::new(),
                }],
            }
        })
        .collect();
    StoredText { lines, ..text }
}

#[test]
fn a_layout_matches_its_own_lines() {
    let session = session();
    let texts = [
        stored(&session, None, "title"),
        stored(&session, Some("card"), "label"),
    ];
    let geometry = session.text_geometry(&texts).unwrap();
    assert_eq!(geometry.texts.len(), 2);
    assert_eq!(geometry.texts[0].lines.len(), 2);
    assert!(
        geometry.texts.iter().all(|t| t.off() == 0.0),
        "{geometry:#?}"
    );
    assert_eq!(geometry.failing().count(), 0);
    let first = geometry.texts[0].lines[0].stored.as_ref().unwrap();
    assert_eq!(first.characters, 5, "`Keep` and its paragraph end");
    assert!(
        first.start < 0.0,
        "a centred line starts left of the origin"
    );
}

#[test]
fn a_wider_stored_line_fails_by_its_gap() {
    let session = session();
    let mut text = stored(&session, None, "title");
    let segment = &mut text.lines[1].segments[0];
    segment.origin[0] -= 3.0;
    segment.width += 6.0;
    segment.advances.clear();
    let geometry = session.text_geometry(&[text]).unwrap();
    let check = &geometry.texts[0];
    assert!((check.off() - 3.0).abs() < 1e-9, "{check:#?}");
    assert_eq!(check.worst_line(), Some(1));
    assert!((check.lines[1].width_gap().unwrap() + 6.0).abs() < 1e-9);
    assert_eq!(geometry.failing().count(), 1);
}

#[test]
fn lines_broken_elsewhere_or_missing_fail_outright() {
    let session = session();
    let mut moved = stored(&session, None, "title");
    moved.lines[0].segments[0].characters += 1;
    moved.lines[1].segments[0].characters -= 1;
    let mut fewer = stored(&session, None, "title");
    fewer.lines.pop();
    let geometry = session.text_geometry(&[moved, fewer]).unwrap();
    assert_eq!(geometry.texts[0].lines[0].off(), f64::INFINITY);
    let missing = &geometry.texts[1].lines[1];
    assert!(missing.stored.is_none() && missing.ours.is_some());
    assert_eq!(missing.width_gap(), None);
    assert_eq!(geometry.failing().count(), 2);
}

#[test]
fn refuses_layers_the_scene_has_not_or_that_hold_no_text() {
    let session = session();
    let text = |comp: Option<&str>, layer: &str| StoredText {
        comp: comp.map(str::to_owned),
        layer: layer.into(),
        lines: Vec::new(),
    };
    for (comp, layer, says) in [
        (None, "nothing", "unknown layer"),
        (Some("deck"), "label", "unknown composition"),
        (None, "box", "holds no text"),
    ] {
        let error = session.text_geometry(&[text(comp, layer)]).unwrap_err();
        assert!(error.message.contains(says), "{}", error.message);
    }
}
