//! Unit tests for `src/key_path.rs`.

use super::*;
use motion_format::Step;
use rstest::rstest;

fn key(text: &str) -> KeyPath {
    text.parse().unwrap()
}

#[test]
fn a_key_keeps_its_text_and_steps() {
    let k = key("anim.opacity");
    assert_eq!(k.as_str(), "anim.opacity");
    assert_eq!(k.to_string(), "anim.opacity");
    assert_eq!(
        k.steps(),
        [Step::Key("anim".into()), Step::Key("opacity".into())]
    );
    assert_eq!(k.first(), "anim");
    assert_eq!(key("fill").first(), "fill");
}

#[rstest]
#[case(0, "")]
#[case(1, "a")]
#[case(2, "a.bc")]
#[case(3, "a.bc.d")]
fn prefix_is_the_text_of_the_leading_segments(#[case] depth: usize, #[case] prefix: &str) {
    assert_eq!(key("a.bc.d").prefix(depth), prefix);
}

#[rstest]
#[case::parent_and_child("anim", "anim.x", true)]
#[case::child_and_parent("anim.x", "anim", true)]
#[case::same_key("anim.x", "anim.x", true)]
#[case::shared_text_prefix("anim", "animal", false)]
#[case::siblings("anim.x", "anim.y", false)]
fn keys_overlap_segment_by_segment(#[case] a: &str, #[case] b: &str, #[case] overlap: bool) {
    assert_eq!(key(a).overlaps(&key(b)), overlap);
}

#[rstest]
#[case::empty_segment("a..b", Problem::InvalidKey("a..b"))]
#[case::operator("delay+", Problem::Operator('+'))]
fn a_malformed_key_is_the_edit_grammar_error(#[case] text: &str, #[case] problem: Problem) {
    assert_eq!(text.parse::<KeyPath>(), Err(problem.diagnostic()));
}

/// Property tests: parsing a key must reject bad text with an error, never
/// panic.
mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn key_any_string(s in ".*") {
            let _ = s.parse::<KeyPath>();
        }

        #[test]
        fn key_dots_and_names(s in "[a-z.+_-]{0,20}") {
            if let Ok(k) = s.parse::<KeyPath>() {
                prop_assert_eq!(k.as_str(), s.as_str());
                prop_assert_eq!(k.prefix(k.steps().len()), s.as_str());
            }
        }
    }
}

#[test]
fn a_selector_picks_a_list_element_by_id_or_index() {
    let k = key("fx[glow].radius");
    assert_eq!(k.first(), "fx");
    assert_eq!(k.prefix(2), "fx[glow]");
    assert_eq!(k.prefix(3), "fx[glow].radius");
    assert_eq!(key("shapes[0]").prefix(2), "shapes[0]");
    assert!(key("fx").overlaps(&key("fx[glow].radius")));
    assert!(!key("fx[glow]").overlaps(&key("fx[blur]")));
    for bad in ["fx[]", "fx[Glow]", "[0]", "fx.[0]"] {
        assert!(bad.parse::<KeyPath>().is_err(), "{bad}");
    }
}
