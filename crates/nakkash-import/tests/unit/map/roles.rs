//! Unit tests for `src/map/roles.rs`.

use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::map::{Options, import};
use crate::test_support::build::{comp, layer, project};

#[rstest]
#[case::named("@headline Big Title", Some("headline"), "Big Title")]
#[case::alone("@logo", Some("logo"), "logo")]
#[case::not_a_role("@YourChannel", None, "@YourChannel")]
#[case::not_a_known_role("@promo Big Title", None, "@promo Big Title")]
#[case::plain("Title", None, "Title")]
fn a_name_may_carry_a_role(#[case] name: &str, #[case] role: Option<&str>, #[case] rest: &str) {
    assert_eq!(named(name), (role.map(str::to_owned), rest));
}

#[rstest]
#[case("title-02", true)]
#[case("main-title", true)]
#[case("subtitle", false)]
#[case("titles", false)]
fn a_word_is_one_part_of_an_id(#[case] id: &str, #[case] yes: bool) {
    assert_eq!(matches(id, &["title"]), yes);
}

/// The roles the import proposes for Main's layers, named `names`.
fn proposed(names: &[&str]) -> (Vec<(String, String)>, serde_json::Value) {
    let layers = (1..)
        .zip(names)
        .map(|(id, name)| layer(id, name, vec![]))
        .collect();
    let imported = import(
        &project(vec![(5, "Main", comp(layers, 2.0))]),
        &Options::default(),
    )
    .expect("it imports");
    let roles = imported
        .mapping
        .roles
        .iter()
        .map(|r| (r.role.clone(), r.address.clone()))
        .collect();
    (roles, imported.json["roles"].clone())
}

#[test]
fn a_named_layer_and_a_lone_match_are_proposed() {
    let (roles, written) = proposed(&["@cta Shop Now", "Logo", "Title", "Bg"]);
    assert_eq!(
        roles,
        [
            ("cta".to_owned(), "shop-now".to_owned()),
            ("headline".to_owned(), "title".to_owned()),
            ("logo".to_owned(), "logo".to_owned()),
            ("background".to_owned(), "bg".to_owned()),
        ]
    );
    assert_eq!(written["cta"], "shop-now");
}

#[test]
fn two_matches_propose_nothing() {
    let (roles, written) = proposed(&["Title 1", "Title 2"]);
    assert!(roles.is_empty(), "{roles:?}");
    assert!(written.is_null());
}
