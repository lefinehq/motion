//! Unit tests for `src/report.rs`.

use super::*;
use crate::test_support::sample_project;

/// `pairs` as a count map.
fn map<const N: usize>(pairs: [(&str, usize); N]) -> BTreeMap<String, usize> {
    pairs.map(|(name, count)| (name.to_owned(), count)).into()
}

#[test]
fn counts_items_layers_effects_opaque_values_and_skipped_chunks() {
    let report = sample_project().report();
    assert_eq!(
        report,
        Report {
            items: map([("composition", 1), ("folder", 1), ("footage", 1)]),
            layers: map([("null", 1), ("text", 1)]),
            effects: map([("ADBE Slider Control", 1)]),
            opaque: map([("text_document", 1)]),
            skipped: map([("LIST:Ewst", 1), ("fvdv", 2)]),
        }
    );
}

#[test]
fn prints_one_line_per_count() {
    let report = Report {
        layers: map([("shape", 3), ("text", 1)]),
        ..Report::default()
    };
    assert_eq!(
        report.to_string(),
        "report\n  items: none\n  layers: shape 3, text 1\n  effects: none\n  opaque: none\n  skipped: none\n"
    );
}
