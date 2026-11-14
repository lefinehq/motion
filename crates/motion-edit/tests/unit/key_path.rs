//! Unit tests for `src/key_path.rs`.

use super::*;

fn key(text: &str) -> KeyPath {
    text.parse().unwrap()
}

#[test]
fn a_key_keeps_its_text_and_segments() {
    let k = key("anim.opacity");
    assert_eq!(k.as_str(), "anim.opacity");
    assert_eq!(k.to_string(), "anim.opacity");
    assert_eq!(k.segments(), ["anim", "opacity"]);
    assert_eq!(k.first(), "anim");
    assert_eq!(key("fill").first(), "fill");
}

#[test]
fn prefix_is_the_text_of_the_leading_segments() {
    let k = key("a.bc.d");
    assert_eq!(k.prefix(0), "");
    assert_eq!(k.prefix(1), "a");
    assert_eq!(k.prefix(2), "a.bc");
    assert_eq!(k.prefix(3), "a.bc.d");
}

#[test]
fn keys_overlap_segment_by_segment() {
    assert!(key("anim").overlaps(&key("anim.x")));
    assert!(key("anim.x").overlaps(&key("anim")));
    assert!(key("anim.x").overlaps(&key("anim.x")));
    assert!(!key("anim").overlaps(&key("animal")));
    assert!(!key("anim.x").overlaps(&key("anim.y")));
}

#[test]
fn a_malformed_key_is_the_edit_grammar_error() {
    assert_eq!(
        "a..b".parse::<KeyPath>().unwrap_err().to_string(),
        "invalid key `a..b`: each dot-separated part must be non-empty letters, digits, `-` or `_`; expected `<key>[.<key>…]=<value>`"
    );
    assert_eq!(
        "delay+".parse::<KeyPath>().unwrap_err().to_string(),
        "operator `+=` is not supported yet; expected `<key>=<value>`"
    );
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
                prop_assert_eq!(k.prefix(k.segments().len()), s.as_str());
            }
        }
    }
}
