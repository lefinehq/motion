//! Unit tests for `src/role.rs`.

use nakkash_format::Kind;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use super::*;

fn doc() -> Document {
    Document::from_value(json!({"nakkash":1,"size":"200x100","dur":"1s",
        "slots":{"accent":"#E63946"},
        "roles":{"headline":"title","accent":"$accent"},
        "layers":[{"id":"title","text":"Launch","fill":"$accent"}]}))
    .unwrap()
}

#[rstest]
#[case::layer("@headline text=\"Sale\"", "title text=\"Sale\"")]
#[case::slot("@accent=#2A9D8F", "$accent=#2A9D8F")]
#[case::spaced("  @headline opacity=0.5", "title opacity=0.5")]
#[case::no_role("title text=x", "title text=x")]
fn a_role_target_is_what_it_names(#[case] line: &str, #[case] meant: &str) {
    assert_eq!(resolve_role(&doc(), line).unwrap(), meant);
}

#[rstest]
#[case::unknown("@cta text=x", Kind::UnknownName)]
#[case::layer_given_a_value("@headline=Sale", Kind::Invalid)]
#[case::slot_given_keys("@accent fill=#fff", Kind::Invalid)]
fn a_role_that_cannot_take_the_edit_is_refused(#[case] line: &str, #[case] kind: Kind) {
    assert_eq!(resolve_role(&doc(), line).unwrap_err().kind, kind);
}

#[test]
fn an_edit_line_goes_through_its_role() {
    let edited = edit_line(&doc(), "@accent=#2A9D8F").unwrap().into_raw();
    assert_eq!(edited["slots"]["accent"], json!("#2A9D8F"));
    let edited = edit_line(&doc(), "@headline text=\"Sale\"")
        .unwrap()
        .into_raw();
    assert_eq!(edited["layers"][0]["text"], json!("Sale"));
}
