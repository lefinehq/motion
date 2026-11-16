//! Unit tests for `src/layout.rs`.

use super::*;

fn line(x: f64, baseline: f64, width: f64) -> Line {
    Line {
        paragraph: 0,
        range: 0..0,
        characters: 0,
        x,
        baseline,
        width,
        ascent: 8.0,
        descent: 2.0,
        advances: Vec::new(),
    }
}

#[test]
fn bounds_cover_every_line_box() {
    let layout = TextLayout {
        lines: vec![line(-5.0, 0.0, 20.0), line(3.0, 12.0, 40.0)],
        runs: Vec::new(),
    };
    assert_eq!(layout.bounds(), Rect::new(-5.0, -8.0, 43.0, 14.0));
    assert_eq!(layout.size(), Size::new(48.0, 22.0));
}

#[test]
fn no_lines_cover_nothing() {
    let layout = TextLayout {
        lines: Vec::new(),
        runs: Vec::new(),
    };
    assert_eq!(layout.bounds(), Rect::ZERO);
    assert_eq!(layout.size(), Size::ZERO);
}

#[test]
fn inks_cover_each_glyph_s_outline_and_nothing_for_a_space() {
    let mut typesetter = crate::Typesetter::new(crate::FontSet::builtin());
    let style =
        crate::CharacterStyle::new(crate::FontQuery::PostScript("InterVariable".into()), 100.0);
    let layout = typesetter
        .layout(&crate::StyledText::plain("Hy x", style))
        .unwrap();
    let run = &layout.runs[0];
    let inks = run.inks();
    assert_eq!(inks.len(), run.glyphs.len());
    let [h, y, space, x] = inks.as_slice() else {
        panic!("four glyphs: {inks:?}");
    };
    let (h, y, x) = (h.unwrap(), y.unwrap(), x.unwrap());
    assert_eq!(*space, None);
    // Y grows downwards from the baseline at 0: a capital stands above it,
    // a descender reaches below, and a lower-case x is shorter than H.
    assert!(h.y0 < -60.0 && h.y1.abs() < 1.0, "{h:?}");
    assert!(y.y1 > 15.0, "{y:?}");
    assert!(x.y0 > h.y0 + 15.0, "{x:?}");
    // Each sits after its glyph's origin, inside its advance.
    let g = &run.glyphs[3];
    assert!(x.x0 >= g.x && x.x1 <= g.x + g.advance, "{x:?} {g:?}");
}
