use rstest::rstest;

use super::*;
use crate::test_support::sample_document;

#[rstest]
#[case::chars("char", 8)]
#[case::chars_without_spaces("char-no-space", 6)]
#[case::words("word", 4)]
#[case::paragraphs_without_a_layout("line", 2)]
fn parts_are_counted_as_the_scene_counts_them(#[case] by: &str, #[case] expected: usize) {
    let document = TextDocument {
        text: "ab c\nde f".into(),
        lines: Vec::new(),
        ..sample_document()
    };
    assert_eq!(total(&document, by), expected);
}
