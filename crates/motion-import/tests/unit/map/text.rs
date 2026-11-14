//! Unit tests for `src/map/text.rs`: style keys against the scene's defaults.

use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::test_support::sample_document;

/// The character keys of the sample's style with `change` applied that a
/// scene must write: those not at the format's default.
fn written(change: impl FnOnce(&mut CharacterStyle)) -> Vec<(&'static str, Json)> {
    let mut style = sample_document().characters[0].style.clone();
    change(&mut style);
    character(&style)
        .into_iter()
        .filter(|(name, value)| !is_default(name, value))
        .collect()
}

/// The value written for `name`, if any.
fn value_of(keys: &[(&'static str, Json)], name: &str) -> Option<Json> {
    keys.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.clone())
}

#[test]
fn a_stroke_of_width_zero_keeps_its_width() {
    let keys = written(|style| style.stroke_width = 0.0);
    assert_eq!(value_of(&keys, "stroke-width"), Some(json!(0)));
}

#[test]
fn a_stroke_at_the_default_width_leaves_the_width_out() {
    let keys = written(|style| style.stroke_width = 1.0);
    assert_eq!(value_of(&keys, "stroke-width"), None);
    assert!(value_of(&keys, "stroke").is_some());
}

#[test]
fn no_stroke_writes_nothing_about_strokes() {
    let keys = written(|style| style.stroke = None);
    assert_eq!(value_of(&keys, "stroke"), None);
    assert_eq!(value_of(&keys, "stroke-width"), None);
}

#[rstest]
#[case::left("left", true)]
#[case::centre("center", false)]
fn only_paragraph_keys_off_their_default_are_written(#[case] align: &str, #[case] left_out: bool) {
    let mut style = sample_document().paragraphs[0].style;
    style.justification = match align {
        "left" => Justification::Left,
        _ => Justification::Center,
    };
    style.first_line_indent = 12.0;
    let keys = paragraph(&style);
    assert_eq!(keys.get("align").is_none(), left_out);
    assert_eq!(keys.get("indent-first"), Some(&json!(12)));
    assert_eq!(keys.get("space-after"), None);
}
