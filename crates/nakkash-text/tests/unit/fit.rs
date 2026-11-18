//! Unit tests for `src/fit.rs`.

use nakkash_format::Kind;
use rstest::rstest;

use super::*;
use crate::set::{FontQuery, FontSet};
use crate::style::{CharacterStyle, Justification, Leading};

const REGULAR: &str = "InterVariable";

fn setter() -> Typesetter {
    Typesetter::new(FontSet::builtin())
}

fn lay(text: &StyledText) -> TextLayout {
    setter().layout(text).expect("the text lays out")
}

/// `words` in Inter at `size`, as point text that fits as `fit` says.
fn point(words: &str, size: f64, fit: Option<Fit>) -> StyledText {
    let style = CharacterStyle::new(FontQuery::PostScript(REGULAR.into()), size);
    let mut text = StyledText::plain(words, style);
    text.fit = fit;
    text
}

/// `text` centred.
fn centred(mut text: StyledText) -> StyledText {
    for paragraph in &mut text.paragraphs {
        paragraph.justification = Justification::Center;
    }
    text
}

/// `words` in Inter at `size`, as box text in a 300 by `height` box at
/// (10, 20).
fn boxed(words: &str, size: f64, height: f64, fit: Option<Fit>) -> StyledText {
    let mut text = point(words, size, fit);
    text.frame = TextFrame::Box {
        left: 10.0,
        top: 20.0,
        width: 300.0,
        height,
    };
    text
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Whether `a` and `b` agree to the single precision the shaper sets sizes
/// and line metrics in.
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-5 * b.abs().max(1.0)
}

#[test]
fn a_text_that_fits_is_set_exactly_as_written() {
    for fit in [
        Fit::Shrink {
            width: 1000.0,
            height: 1000.0,
        },
        Fit::Wrap {
            width: 1000.0,
            height: 1000.0,
        },
    ] {
        let fitted = lay(&point("Hi there", 60.0, Some(fit)));
        assert_eq!(fitted, lay(&point("Hi there", 60.0, None)), "{fit:?}");
        assert_eq!(fitted.scale, 1.0);
    }
    let fitted = lay(&boxed("Hi there", 60.0, 400.0, Some(Fit::Box)));
    assert_eq!(fitted, lay(&boxed("Hi there", 60.0, 400.0, None)));
}

#[test]
fn a_long_word_shrinks_to_its_width_about_the_middle_of_its_line() {
    let fit = Fit::Shrink {
        width: 300.0,
        height: 400.0,
    };
    let written = lay(&point("Extraordinarily", 100.0, None));
    let fitted = lay(&point("Extraordinarily", 100.0, Some(fit)));
    assert!(written.size().width > 300.0);
    let size = fitted.size();
    // The largest scale that fits, to the search's precision.
    assert!(size.width <= 300.0, "{size:?}");
    assert!(size.width > 300.0 * (1.0 - 2.0 * PRECISION), "{size:?}");
    assert!(fitted.scale < 1.0 && fitted.scale > 0.0);
    assert!(near(fitted.runs[0].size, 100.0 * fitted.scale));
    // The vertical middle of the line stays, and it still starts at the origin.
    assert!(close(
        fitted.bounds().center().y,
        written.bounds().center().y
    ));
    assert_eq!(fitted.lines[0].x, 0.0);
    // The same text fits the same way every time.
    assert_eq!(fitted, lay(&point("Extraordinarily", 100.0, Some(fit))));
}

#[test]
fn a_text_too_tall_shrinks_to_its_height() {
    let fit = Fit::Shrink {
        width: 2000.0,
        height: 150.0,
    };
    let fitted = lay(&point("one\ntwo\nthree", 100.0, Some(fit)));
    assert_eq!(fitted.lines.len(), 3);
    let height = fitted.size().height;
    assert!(height <= 150.0 && height > 150.0 * (1.0 - 2.0 * PRECISION));
}

#[test]
fn a_shrunk_centred_line_stays_centred_on_the_origin() {
    let fit = Fit::Shrink {
        width: 300.0,
        height: 400.0,
    };
    let fitted = lay(&centred(point("Extraordinarily", 100.0, Some(fit))));
    let line = &fitted.lines[0];
    assert!(close(line.x, -line.width / 2.0));
}

#[test]
fn every_length_of_the_style_shrinks_with_the_size() {
    let fit = Fit::Shrink {
        width: 300.0,
        height: 1000.0,
    };
    let mut text = point("Extraordinarily\nwords", 100.0, Some(fit));
    text.runs[0].style.leading = Leading::Px(150.0);
    text.runs[0].style.baseline_shift = 10.0;
    let fitted = lay(&text);
    let s = fitted.scale;
    let step = fitted.lines[1].baseline - fitted.lines[0].baseline;
    assert!(near(step, 150.0 * s), "{step} at {s}");
    let glyph = &fitted.runs[0].glyphs[0];
    assert!(near(fitted.lines[0].baseline - glyph.y, 10.0 * s));
}

#[test]
fn wrap_breaks_point_text_at_its_width_aligned_about_the_origin() {
    let fit = Fit::Wrap {
        width: 260.0,
        height: 1000.0,
    };
    let words = "breaks its lines like box text";
    let fitted = lay(&centred(point(words, 40.0, Some(fit))));
    assert!(fitted.lines.len() > 1, "{:?}", fitted.lines);
    assert_eq!(fitted.scale, 1.0);
    // The first baseline is where point text's is.
    assert_eq!(fitted.lines[0].baseline, 0.0);
    for line in &fitted.lines {
        assert!(line.width <= 260.0);
        assert!(close(line.x, -line.width / 2.0));
    }
    let as_box = lay(&{
        let mut text = point(words, 40.0, None);
        text.frame = TextFrame::Box {
            left: 0.0,
            top: 0.0,
            width: 260.0,
            height: 1000.0,
        };
        text
    });
    let ranges = |l: &TextLayout| l.lines.iter().map(|l| l.range.clone()).collect::<Vec<_>>();
    assert_eq!(ranges(&fitted), ranges(&as_box));
}

#[test]
fn wrapped_text_too_tall_shrinks_keeping_its_top() {
    let words = "breaks its lines like box text and then some more";
    let tall = Fit::Wrap {
        width: 260.0,
        height: 1000.0,
    };
    let short = Fit::Wrap {
        width: 260.0,
        height: 100.0,
    };
    let written = lay(&point(words, 40.0, Some(tall)));
    let fitted = lay(&point(words, 40.0, Some(short)));
    assert!(written.size().height > 100.0);
    assert!(fitted.scale < 1.0);
    let size = fitted.size();
    assert!(size.height <= 100.0 && size.width <= 260.0, "{size:?}");
    // Set smaller, the words wrap again at the same width, so fewer lines.
    assert!(fitted.lines.len() < written.lines.len());
    assert!(close(fitted.bounds().y0, written.bounds().y0));
}

#[test]
fn box_text_shrinks_until_its_lines_fit_its_box() {
    let words = "many words that wrap inside the box and run past its foot";
    let written = lay(&boxed(words, 40.0, 120.0, None));
    let fitted = lay(&boxed(words, 40.0, 120.0, Some(Fit::Box)));
    assert!(written.size().height > 120.0);
    assert!(fitted.scale < 1.0);
    let bounds = fitted.bounds();
    assert!(bounds.height() <= 120.0 && bounds.height() > 120.0 * 0.9);
    // Still in its box: from the top, one ascent down, wrapping at its width.
    let first = &fitted.lines[0];
    assert!(close(first.baseline, 20.0 + first.ascent));
    assert!(fitted.lines.iter().all(|l| l.x >= 10.0 && l.width <= 300.0));
}

#[rstest]
#[case::box_fit_on_point_text(point("a", 10.0, Some(Fit::Box)), Kind::Conflict)]
#[case::shrink_with_a_size_on_box_text(
    boxed("a", 10.0, 100.0, Some(Fit::Shrink { width: 10.0, height: 10.0 })),
    Kind::Conflict
)]
#[case::wrap_on_box_text(
    boxed("a", 10.0, 100.0, Some(Fit::Wrap { width: 10.0, height: 10.0 })),
    Kind::Conflict
)]
#[case::no_width(point("a", 10.0, Some(Fit::Shrink { width: 0.0, height: 10.0 })), Kind::OutOfRange)]
#[case::endless_height(
    point("a", 10.0, Some(Fit::Wrap { width: 10.0, height: f64::INFINITY })),
    Kind::OutOfRange
)]
fn a_fit_that_does_not_suit_its_text_is_refused(#[case] text: StyledText, #[case] kind: Kind) {
    let error = setter().layout(&text).unwrap_err();
    assert_eq!(
        (error.kind, error.path_text().as_deref()),
        (kind, Some("fit"))
    );
}

#[test]
fn a_text_fits_the_size_of_its_own_lines_as_written() {
    let words = point("one\ntwo three", 50.0, None);
    let size = lay(&words).size();
    let fitted = |fit: Fit| {
        let mut text = words.clone();
        text.fit = Some(fit);
        text
    };
    let fit = |width: f64, height: f64| fitted(Fit::Shrink { width, height });
    // Exactly its size, and its size rounded up to two decimals, fit as
    // written, shrinking or wrapping.
    let rounded = |v: f64| (v * 100.0).ceil() / 100.0;
    for (width, height) in [
        (size.width, size.height),
        (rounded(size.width), rounded(size.height)),
    ] {
        assert_eq!(lay(&fit(width, height)), lay(&words));
        assert_eq!(lay(&fitted(Fit::Wrap { width, height })), lay(&words));
    }
    // The layout as written leaves a fit out.
    let mut setter = setter();
    let squeezed = fit(size.width / 2.0, size.height);
    assert_eq!(setter.layout_as_written(&squeezed).unwrap(), lay(&words));
    assert!(setter.layout(&squeezed).unwrap().scale < 1.0);
}
