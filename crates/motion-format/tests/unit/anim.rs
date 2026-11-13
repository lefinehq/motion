//! Unit tests for `src/anim.rs`.

use super::*;
use crate::parse_document;
use serde_json::json;

fn anim(v: Value) -> Result<Vec<AnimTrack>, FormatError> {
    parse_anim(&v, "layers[1].anim", 30.0)
}

fn err(v: Value) -> String {
    anim(v).unwrap_err().to_string()
}

#[test]
fn string_and_array_forms_give_identical_keys() {
    let s = anim(json!({"x": "0 -> 200 @0-30f ease-out"})).unwrap();
    let a = anim(json!({"x": [["0f", 0, "ease-out"], ["30f", 200]]})).unwrap();
    assert_eq!(s, a);
    assert_eq!(s[0].keys.len(), 2);
}

#[test]
fn seconds_stay_seconds() {
    let t = anim(json!({"opacity": "0 -> 1 @1-1.5s"})).unwrap();
    assert_eq!(t[0].keys[0].time, Time::Seconds(1.0));
    assert_eq!(t[0].keys[1].time, Time::Seconds(1.5));
    let t = anim(json!({"opacity": "0 -> 1 @0.5s-1s"})).unwrap();
    assert_eq!(t[0].keys[0].time, Time::Seconds(0.5));
    let t = anim(json!({"opacity": "0 -> 1 @0f-12f"})).unwrap();
    assert_eq!(t[0].keys[1].time, Time::Frames(12.0));
    let t = anim(json!({"opacity": "0 -> 1 @0f-1s"})).unwrap();
    assert_eq!(t[0].keys[0].time, Time::Frames(0.0));
}

#[test]
fn default_curve_is_linear() {
    let t = anim(json!({"x": "0 -> 1 @0-5f", "y": [[0, 0], [5, 1]]})).unwrap();
    for track in &t {
        assert!(track.keys.iter().all(|k| k.curve == Curve::Linear));
    }
}

#[test]
fn bare_numbers_are_frames_and_order_follows_object() {
    let t = anim(json!({"y": [[2, 1]], "x": [[0, 1], [4.5, 2]]})).unwrap();
    assert_eq!(t[0].property, property("y").unwrap().0);
    assert_eq!(t[1].property, property("x").unwrap().0);
    assert_eq!(t[1].keys[1].time, Time::Frames(4.5));
}

#[test]
fn unknown_property_lists_names() {
    let e = err(json!({"glow": "0 -> 1 @0-5f"}));
    assert!(
        e.starts_with("layers[1].anim.glow: unknown property `glow`"),
        "{e}"
    );
    assert!(e.contains(&property_names()), "{e}");
}

#[test]
fn out_of_range_value() {
    let e = err(json!({"opacity": [[0, 0], [5, 1], [9, 2]]}));
    assert_eq!(
        e,
        "layers[1].anim.opacity[2]: value 2 is outside 0..=1; expected a number from 0 to 1"
    );
    let e = err(json!({"opacity": "0 -> 2 @0-5f"}));
    assert!(
        e.starts_with("layers[1].anim.opacity: value 2 is outside 0..=1"),
        "{e}"
    );
}

#[test]
fn string_form_errors() {
    for (text, needle) in [
        ("0 1 @0-5f", "missing `->`"),
        ("0 -> 1", "missing `@<start>-<end>`"),
        ("0 -> 1 @0-5", "needs a unit"),
        ("0 -> 1 @5f-0f", "starts after it ends"),
        ("0 -> 1 @0-5f bouncy", "`bouncy` is not a curve"),
        ("a -> 1 @0-5f", "`a` is not a number"),
        ("0 -> 1 @0-5f ease ease", "unexpected"),
        ("0 -> 1 @0-5f cubic(1,2)", "four numbers"),
        ("0 -> 1 @05f", "no `-`"),
    ] {
        let e = err(json!({"x": text}));
        assert!(e.starts_with("layers[1].anim.x: "), "{e}");
        assert!(e.contains(needle), "{text}: {e}");
        assert!(e.contains("expected"), "{e}");
    }
    let e = err(json!({"x": "0 1 @0-5f"}));
    assert!(
        e.contains("\"<from> -> <to> @<start>-<end> [<curve>]\", e.g. \"0 -> 1 @0-12f ease-out\""),
        "{e}"
    );
}

#[test]
fn array_form_errors() {
    assert!(err(json!({"x": []})).contains("anim.x: needs at least one key"));
    assert!(err(json!({"x": [[5, 0], [2, 1]]})).starts_with("layers[1].anim.x[1]: key is earlier"));
    assert!(err(json!({"x": [[0]]})).starts_with("layers[1].anim.x[0]: must be a list of 2 or 3"));
    assert!(err(json!({"x": [[-1, 0]]})).starts_with("layers[1].anim.x[0]: invalid time"));
    assert!(err(json!({"x": [["12", 0]]})).starts_with("layers[1].anim.x[0]: "));
    assert!(err(json!({"x": [[0, "a"]]})).contains("invalid value"));
    assert!(err(json!({"x": [[0, 1, "nope"]]})).contains("unknown curve"));
    assert!(err(json!({"x": [[0, 1, 3]]})).contains("invalid curve"));
    assert!(err(json!({"x": 3})).contains("must be a string or a list"));
    assert!(err(json!([1])).starts_with("layers[1].anim: must be an object"));
}

#[test]
fn mixed_units_order_uses_fps() {
    assert!(anim(json!({"x": [["1s", 0], [31, 1]]})).is_ok());
    assert!(anim(json!({"x": [["1s", 0], [29, 1]]})).is_err());
}

fn scene(layer: &str) -> Result<crate::Document, FormatError> {
    parse_document(&format!(
        r#"{{"motion":1,"dur":"2s","layers":[{{"id":"a","rect":"10x10",{layer}}}]}}"#
    ))
}

#[test]
fn layer_carries_anim() {
    let d = scene(r#""anim":{"x":"0 -> 5 @0-5f"}"#).unwrap();
    assert_eq!(d.scene.layers[0].anim.len(), 1);
    // A property the preset does not touch is fine next to `in`.
    assert!(scene(r#""in":"fade-up","anim":{"rotate":"0 -> 5 @0-5f"}"#).is_ok());
}

#[test]
fn conflict_with_in_preset() {
    let e = scene(r#""in":"fade-up","anim":{"opacity":"0 -> 1 @0-5f"}"#)
        .unwrap_err()
        .to_string();
    assert_eq!(
        e,
        "layers[0].anim.opacity: `opacity` is already animated by `in` (preset `fade-up`); expected remove it from `in` or put the whole animation in `anim`"
    );
}

#[test]
fn conflict_uses_scene_preset() {
    let doc = r#"{"motion":1,"dur":"2s","presets":{"fade-up":{"rotate":0}},"layers":[{"id":"a","rect":"1x1","in":"fade-up","anim":{"opacity":"0 -> 1 @0-5f"}}]}"#;
    assert!(parse_document(doc).is_ok());
}

mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn string_form_never_panics(s in ".{0,40}") {
            let spec = property("opacity").unwrap().1;
            let _ = parse_track_string(&s, "p", spec, 30.0);
        }

        #[test]
        fn string_form_shaped_input_never_panics(
            a in "[-0-9.a-z]{0,5}", b in "[-0-9.a-z]{0,5}", r in "[-@0-9.fs]{0,10}", c in "[a-z(),0-9.-]{0,12}"
        ) {
            let spec = property("x").unwrap().1;
            let _ = parse_track_string(&format!("{a} -> {b} @{r} {c}"), "p", spec, 30.0);
        }

        #[test]
        fn parse_anim_never_panics(v in arb_json()) {
            let _ = parse_anim(&v, "anim", 30.0);
            let _ = parse_anim(&json!({"opacity": v.clone(), "x": v}), "anim", 24.0);
        }
    }

    fn arb_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::from),
            any::<f64>().prop_map(|f| json!(f)),
            ".{0,12}".prop_map(Value::from),
            "[0-9.]{1,4}[fs]".prop_map(Value::from),
        ];
        leaf.prop_recursive(3, 24, 4, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
                proptest::collection::vec(("[a-z]{1,8}", inner), 0..3)
                    .prop_map(|kv| Value::Object(kv.into_iter().collect())),
            ]
        })
    }
}
