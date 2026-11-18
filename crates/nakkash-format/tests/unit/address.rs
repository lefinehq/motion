//! Unit tests for `src/address.rs`.

use pretty_assertions::assert_eq;
use proptest::prelude::*;
use rstest::rstest;

use super::*;

fn id(text: &str) -> Id {
    text.parse().unwrap()
}

#[rstest]
#[case::top_level("card", vec!["card"], vec![])]
#[case::nested("card/title", vec!["card", "title"], vec![])]
#[case::scene("scene", vec![], vec![])]
#[case::scene_key("scene.fps", vec![], vec![Step::Key("fps".into())])]
#[case::keys(
    "card/title.fx[glow].radius",
    vec!["card", "title"],
    vec![Step::Key("fx".into()), Step::Id(id("glow")), Step::Key("radius".into())]
)]
#[case::index("title.runs[0]", vec!["title"], vec![Step::Key("runs".into()), Step::Index(0)])]
#[case::longer_than_scene("scenery", vec!["scenery"], vec![])]
#[case::scene_inside("card/scene", vec!["card", "scene"], vec![])]
fn reads_an_address(#[case] text: &str, #[case] layers: Vec<&str>, #[case] keys: Vec<Step>) {
    let address: Address = text.parse().unwrap();
    assert_eq!(
        address.layers,
        layers.into_iter().map(id).collect::<Vec<_>>()
    );
    assert_eq!(address.keys, keys);
    assert_eq!(address.to_string(), text);
}

#[rstest]
#[case::empty("")]
#[case::upper("Card")]
#[case::trailing_slash("card/")]
#[case::scene_then_layer("scene/card")]
#[case::unclosed("card.fx[glow")]
#[case::empty_key("card.")]
#[case::space("card title")]
fn refuses_what_is_not_an_address(#[case] text: &str) {
    let error = text.parse::<Address>().unwrap_err();
    assert_eq!(error.kind, Kind::Syntax);
    assert_eq!(error.expected.as_deref(), Some(EXPECTED));
}

#[rstest]
#[case::letters("card", true)]
#[case::digits_dash_underscore("a-1_b", true)]
#[case::leading_digit("1a", false)]
#[case::upper("A", false)]
#[case::slash("a/b", false)]
#[case::empty("", false)]
fn the_id_rule(#[case] text: &str, #[case] valid: bool) {
    assert_eq!(Id::is_valid(text), valid);
    assert_eq!(text.parse::<Id>().is_ok(), valid);
}

#[test]
fn an_invalid_id_says_the_rule() {
    let error = "1a".parse::<Id>().unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert_eq!(error.expected.as_deref(), Some(ID_RULE));
}

fn an_id() -> impl Strategy<Value = Id> {
    "[a-z][a-z0-9_-]{0,6}".prop_map(|s: String| Id(s.into()))
}

fn a_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        "[a-z][a-z0-9-]{0,6}".prop_map(Step::Key),
        an_id().prop_map(Step::Id),
        (0usize..100).prop_map(Step::Index),
    ]
}

/// An address that prints: selectors only right after a key.
fn an_address() -> impl Strategy<Value = Address> {
    (
        prop::collection::vec(an_id(), 0..4),
        prop::collection::vec(("[a-z][a-z0-9-]{0,6}", prop::option::of(a_step())), 0..4),
    )
        .prop_filter("a layer path never starts with `scene`", |(layers, _)| {
            layers.first().is_none_or(|id| id.as_ref() != SCENE)
        })
        .prop_map(|(layers, pairs)| Address {
            layers,
            keys: pairs
                .into_iter()
                .flat_map(|(key, selector)| {
                    let selector = selector.filter(|s| !matches!(s, Step::Key(_)));
                    std::iter::once(Step::Key(key)).chain(selector)
                })
                .collect(),
        })
}

proptest! {
    #[test]
    fn never_panics(text in "\\PC{0,40}") {
        let _ = text.parse::<Address>();
    }

    #[test]
    fn prints_then_reads_back(address in an_address()) {
        prop_assert_eq!(address.to_string().parse::<Address>().unwrap(), address);
    }
}

#[test]
fn a_key_path_has_no_leading_dot() {
    let read = |text: &str| crate::grammar::parse_whole(text, key_path, "key", "a key");
    assert_eq!(
        read("fx[glow].radius").unwrap(),
        [
            Step::Key("fx".into()),
            Step::Id(id("glow")),
            Step::Key("radius".into())
        ]
    );
    assert_eq!(
        read("layers[0]").unwrap(),
        [Step::Key("layers".into()), Step::Index(0)]
    );
    for bad in ["", ".fill", "fill.", "fx[]", "[0]"] {
        assert!(read(bad).is_err(), "{bad}");
    }
}
