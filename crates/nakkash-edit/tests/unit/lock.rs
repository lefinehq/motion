//! Unit tests for `src/lock.rs`.

use nakkash_format::Kind;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use crate::edit_line;
use nakkash_format::Document;

/// A master whose logo, end card and accent are locked, with its palette
/// and typefaces.
fn master() -> Document {
    Document::from_value(json!({"nakkash":1,"size":"200x100","dur":"1s",
        "slots":{"accent":"#E63946","lift":4},
        "roles":{"logo":"logo","headline":"title","accent":"$accent"},
        "locks":["@logo","end","@accent","palette","typefaces"],
        "comps":{"card":{"size":"50x50","dur":"1s","layers":[{"id":"cta","text":"Buy","font":"Inter 20"}]}},
        "layers":[
            {"id":"logo","rect":"10x10","fill":"#FFFFFF"},
            {"id":"title","text":"Hi","font":"Inter 40 700","fill":"#E63946"},
            {"id":"end","comp":"card"},
            {"id":"other","comp":"card"}]}))
    .unwrap()
}

#[rstest]
#[case::text("@headline text=\"Sale\"")]
#[case::a_colour_it_has("title fill=#FFFFFF")]
#[case::another_weight("title font=\"Inter 40 400\"")]
#[case::an_unlocked_slot("$lift=8")]
#[case::outside_the_lock("other/cta text=\"Go\"")]
fn an_edit_of_what_is_not_locked_is_made(#[case] line: &str) {
    edit_line(&master(), line).unwrap();
}

#[rstest]
#[case::role("@logo opacity=0.5", "`logo`")]
#[case::layer("logo fill=#E63946", "`logo`")]
#[case::inside("end/cta text=\"Go\"", "`end/cta` is inside `end`")]
#[case::override_into_it("end overrides.cta.text=Go", "`end`")]
#[case::its_composition("scene comps.card.layers[cta].text=Go", "`end`")]
#[case::slot("$accent=#FFFFFF", "slot `$accent`")]
#[case::slot_by_role("@accent=#FFFFFF", "slot `$accent`")]
#[case::new_colour("title fill=#00FF00", "`#00FF00` is not one of the scene's colours")]
#[case::new_family(
    "title font=\"Roboto 40\"",
    "`Roboto` is not one of the scene's typefaces"
)]
fn an_edit_that_touches_a_lock_is_refused(#[case] line: &str, #[case] says: &str) {
    let err = edit_line(&master(), line).unwrap_err();
    assert_eq!(err.kind, Kind::Conflict, "{line}: {err}");
    assert!(err.message.contains(says), "{line}: {}", err.message);
}

#[test]
fn the_lock_names_the_role_it_came_from() {
    let err = edit_line(&master(), "logo opacity=0.5").unwrap_err();
    assert_eq!(err.message, "`logo` is locked by `@logo`");
}
