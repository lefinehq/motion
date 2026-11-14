//! Unit tests for `src/style.rs`.

use rstest::rstest;

use super::*;

fn style() -> CharacterStyle {
    CharacterStyle::new(FontQuery::PostScript("InterVariable".into()), 20.0)
}

fn text(text: &str) -> StyledText {
    StyledText::plain(text, style())
}

#[test]
fn plain_text_has_one_run_and_a_style_per_paragraph() {
    let plain = text("a\nbc\n");
    assert_eq!(plain.runs.len(), 1);
    assert_eq!(plain.runs[0].range, 0..5);
    assert_eq!(plain.paragraphs.len(), 3);
    assert_eq!(plain.paragraph_ranges(), [0..1, 2..4, 5..5]);
    assert_eq!(plain.frame, TextFrame::Point);
    assert!(plain.check().is_ok());
    assert_eq!(ParagraphStyle::default().auto_leading, AUTO_LEADING);
}

fn broken(change: fn(&mut StyledText)) -> StyledText {
    let mut styled = text("ab\né");
    styled.runs = vec![
        StyledRun {
            range: 0..2,
            style: style(),
        },
        StyledRun {
            range: 2..5,
            style: style(),
        },
    ];
    change(&mut styled);
    styled
}

#[rstest]
#[case::paragraph_count(broken(|t| t.paragraphs.pop().map(drop).unwrap_or(())), Kind::Invalid, "paragraphs")]
#[case::gap(broken(|t| t.runs[1].range = 3..5), Kind::Invalid, "runs[1].range")]
#[case::empty_run(broken(|t| t.runs[1].range = 2..2), Kind::Invalid, "runs[1].range")]
#[case::split_character(broken(|t| { t.runs[0].range = 0..4; t.runs[1].range = 4..5 }), Kind::Invalid, "runs[0].range")]
#[case::short(broken(|t| { t.runs.pop(); }), Kind::Invalid, "runs")]
#[case::zero_size(broken(|t| t.runs[1].style.size = 0.0), Kind::OutOfRange, "runs[1].style.size")]
#[case::infinite_size(broken(|t| t.runs[0].style.size = f64::INFINITY), Kind::OutOfRange, "runs[0].style.size")]
#[case::tracking(broken(|t| t.runs[0].style.tracking = f64::NAN), Kind::OutOfRange, "runs[0].style.tracking")]
#[case::shift(broken(|t| t.runs[0].style.baseline_shift = f64::NAN), Kind::OutOfRange, "runs[0].style.baseline_shift")]
#[case::leading(broken(|t| t.runs[0].style.leading = Leading::Px(f64::NAN)), Kind::OutOfRange, "runs[0].style.leading")]
#[case::indent(broken(|t| t.paragraphs[1].start_indent = f64::NAN), Kind::OutOfRange, "paragraphs[1].start_indent")]
#[case::box_width(broken(|t| t.frame = TextFrame::Box { left: 0.0, top: 0.0, width: 0.0, height: 5.0 }), Kind::OutOfRange, "frame")]
#[case::small_caps(broken(|t| t.small_caps_scale = f64::NAN), Kind::OutOfRange, "small_caps_scale")]
fn check_names_the_value_that_breaks_a_rule(
    #[case] styled: StyledText,
    #[case] kind: Kind,
    #[case] path: &str,
) {
    let error = styled.check().unwrap_err();
    assert_eq!((error.kind, error.path.as_deref()), (kind, Some(path)));
    assert!(error.expected.is_some());
}

#[test]
fn a_good_text_passes() {
    let mut styled = broken(|_| ());
    styled.frame = TextFrame::Box {
        left: -5.0,
        top: 3.0,
        width: 10.0,
        height: 5.0,
    };
    assert!(styled.check().is_ok());
}
