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
