//! Unit tests for `src/caps.rs`.

use rstest::rstest;

use super::*;
use crate::set::FontQuery;
use crate::style::CharacterStyle;

fn cased(text: &str, caps: Caps) -> Cased {
    let mut style = CharacterStyle::new(FontQuery::PostScript("InterVariable".into()), 10.0);
    style.caps = caps;
    apply(&StyledText::plain(text, style))
}

#[rstest]
#[case::normal(Caps::Normal, "Aßb", [false, false, false])]
#[case::all(Caps::All, "ASSB", [false, false, false])]
#[case::small(Caps::Small, "ASSB", [false, true, true])]
#[case::all_small(Caps::AllSmall, "ASSB", [true, true, true])]
fn capitals_change_the_text_before_it_is_set(
    #[case] caps: Caps,
    #[case] set: &str,
    #[case] small_by_source: [bool; 3],
) {
    let cased = cased("Aßb", caps);
    assert_eq!(cased.text, if caps == Caps::Normal { "Aßb" } else { set });
    let small: Vec<bool> = [0, 1, 3]
        .iter()
        .map(|&at| cased.span_at(cased_at(&cased, at)).small)
        .collect();
    assert_eq!(small, small_by_source);
}

/// The first set byte of the source character at `source` byte.
fn cased_at(cased: &Cased, source: usize) -> usize {
    cased
        .spans
        .iter()
        .find(|s| s.source.start == source)
        .unwrap()
        .set
        .start
}

#[test]
fn set_bytes_map_back_to_their_source() {
    let cased = cased("aßc", Caps::All);
    assert_eq!(cased.text, "ASSC");
    let sources: Vec<_> = cased.spans.iter().map(|s| s.source.clone()).collect();
    assert_eq!(sources, [0..1, 1..3, 1..3, 3..4]);
    assert_eq!(cased.source_of(1..2), 1..3);
    assert_eq!(cased.source_of(0..4), 0..4);
    assert_eq!(cased.source_of(2..2), 1..1);
    // Past the end, the last character answers.
    assert_eq!(cased.span_at(10).source, 3..4);
}

#[test]
fn spans_know_their_run() {
    let mut text = StyledText::plain(
        "ab",
        CharacterStyle::new(FontQuery::PostScript("InterVariable".into()), 10.0),
    );
    text.runs = vec![
        crate::style::StyledRun {
            range: 0..1,
            style: text.runs[0].style.clone(),
        },
        crate::style::StyledRun {
            range: 1..2,
            style: CharacterStyle {
                caps: Caps::All,
                ..text.runs[0].style.clone()
            },
        },
    ];
    let cased = apply(&text);
    assert_eq!(cased.text, "aB");
    assert_eq!(
        cased.spans.iter().map(|s| s.run).collect::<Vec<_>>(),
        [0, 1]
    );
}
