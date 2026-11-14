//! Unit tests for `src/anim.rs`: the string grammar, and tracks read
//! through a whole scene.

use super::*;
use crate::property::Prop;
use crate::{parse_document, scene_from_value};
use motion_diagnostic::Kind;
use serde_json::{Value, json};

/// The tracks of `v` as the `anim` of the second layer of a scene.
fn anim(v: Value) -> Result<Vec<AnimTrack>> {
    let scene = json!({
        "motion": 1,
        "dur": "1s",
        "layers": [{"id": "z", "rect": "1x1"}, {"id": "a", "rect": "1x1", "anim": v}],
    });
    scene_from_value(&scene).map(|mut s| s.layers.remove(1).anim)
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
    assert_eq!(t[0].property, Prop::Y);
    assert_eq!(t[1].property, Prop::X);
    assert_eq!(t[1].keys[1].time, Time::Frames(4.5));
}

#[test]
fn unknown_property_lists_names() {
    let e = err(json!({"glow": "0 -> 1 @0-5f"}));
    assert!(
        e.starts_with("layers[1].anim.glow: unknown property `glow`"),
        "{e}"
    );
    assert!(e.contains("one of x, y, opacity, scale, rotate"), "{e}");
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
        (
            "0 -> 1 @0-5f bouncy",
            "unknown curve `bouncy`; expected one of linear, hold, ease",
        ),
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
fn string_form_messages() {
    let line = |text: &str| text.parse::<TrackLine>().unwrap_err().to_string();
    let form =
        r#"expected "<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out""#;
    assert_eq!(
        line("0 1 @0-5f"),
        format!("missing `->` after the start value in `0 1 @0-5f`; {form}")
    );
    assert_eq!(
        line("0 -> 1 2 @0-5f"),
        format!("missing `@<start>-<end>` after the end value in `0 -> 1 2 @0-5f`; {form}")
    );
    assert_eq!(line("a -> 1 @0-5f"), format!("`a` is not a number; {form}"));
    assert_eq!(
        line("0 -> 1 @0-5f ease ease"),
        format!("unexpected `ease` after the curve; {form}")
    );
    assert_eq!(line(""), format!("`` is not a valid track; {form}"));
    let range = "expected `@<start>-<end>` with a unit, e.g. `@0-30f`, `@0.5s-1s`";
    assert_eq!(
        line("0 -> 1 @05f"),
        format!("`@05f` has no `-` between start and end; {range}")
    );
    assert_eq!(
        line("0 -> 1 @0-5"),
        format!("`@0-5` needs a unit (`f` or `s`) after the end; {range}")
    );
    assert_eq!(
        line("0 -> 1 @x-5f"),
        format!("`@x-5f` is not a valid range; {range}")
    );
    assert_eq!(
        err(json!({"x": "0 -> 1 @5-0f"})),
        format!("layers[1].anim.x: `@5f-0f` starts after it ends; {range}")
    );
}

#[test]
fn ranges_borrow_the_end_unit() {
    let span = |text: &str| text.parse::<Span>().unwrap();
    assert_eq!(
        span("@0-30f"),
        Span {
            start: Time::Frames(0.0),
            end: Time::Frames(30.0)
        }
    );
    assert_eq!(span("@1-1.5s").start, Time::Seconds(1.0));
    assert_eq!(span("@0f-1s").start, Time::Frames(0.0));
    assert!("@-1-5f".parse::<Span>().is_err());
}

#[test]
fn array_form_errors() {
    assert!(err(json!({"x": []})).contains("anim.x: needs at least one key"));
    assert!(err(json!({"x": [[5, 0], [2, 1]]})).starts_with("layers[1].anim.x[1]: key is earlier"));
    assert_eq!(
        err(json!({"x": [[0]]})),
        r#"layers[1].anim.x[0]: invalid length 1; expected [time, value] or [time, value, curve], e.g. ["12f", 1, "ease-out"]"#
    );
    assert_eq!(
        err(json!({"x": [[0, 1, "linear", 4]]})),
        r#"layers[1].anim.x[0]: invalid length 4; expected [time, value] or [time, value, curve], e.g. ["12f", 1, "ease-out"]"#
    );
    assert!(err(json!({"x": [[-1, 0]]})).starts_with("layers[1].anim.x[0]: invalid time -1"));
    assert!(err(json!({"x": [["12", 0]]})).starts_with("layers[1].anim.x[0]: "));
    assert_eq!(
        err(json!({"x": [[true, 0]]})),
        "layers[1].anim.x[0][0]: invalid type: boolean `true`; expected a frame count, or a time such as `12f` or `0.4s`"
    );
    assert_eq!(
        err(json!({"x": [[0, "a"]]})),
        r#"layers[1].anim.x[0][1]: invalid type: string "a"; expected a number"#
    );
    assert!(err(json!({"x": [[0, 1, "nope"]]})).contains("unknown curve"));
    assert_eq!(
        err(json!({"x": [[0, 1, 3]]})),
        "layers[1].anim.x[0][2]: invalid type: integer `3`; expected a curve name such as `ease-out`, or `cubic(x1,y1,x2,y2)`"
    );
    assert_eq!(
        err(json!({"x": [[0, 1, null]]})),
        "layers[1].anim.x[0][2]: invalid type: null; expected a curve name such as `ease-out`, or `cubic(x1,y1,x2,y2)`"
    );
    assert_eq!(
        err(json!({"x": 3})),
        r#"layers[1].anim.x: invalid type: integer `3`; expected "<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out", or a list of [time, value] keys"#
    );
    assert_eq!(
        err(json!([1])),
        r#"layers[1].anim: invalid type: sequence; expected an object of property tracks, e.g. {"opacity":"0 -> 1 @0-12f"}"#
    );
}

#[test]
fn array_form_errors_say_what_kind_of_problem_they_are() {
    let kind = |v: Value| anim(v).unwrap_err().kind;
    assert_eq!(kind(json!([1])), Kind::WrongType);
    assert_eq!(kind(json!({"x": 3})), Kind::WrongType);
    assert_eq!(kind(json!({"x": [5]})), Kind::WrongType);
    assert_eq!(kind(json!({"x": [[true, 0]]})), Kind::WrongType);
    assert_eq!(kind(json!({"x": [[0, "a"]]})), Kind::WrongType);
    assert_eq!(kind(json!({"x": [[0, 1, 3]]})), Kind::WrongType);
    assert_eq!(kind(json!({"x": [[0]]})), Kind::Invalid);
    assert_eq!(kind(json!({"x": [[5, 0], [2, 1]]})), Kind::Invalid);
    assert_eq!(kind(json!({"x": [[-1, 0]]})), Kind::OutOfRange);
    assert_eq!(kind(json!({"opacity": [[0, 2]]})), Kind::OutOfRange);
    assert_eq!(kind(json!({"x": []})), Kind::Missing);
    assert_eq!(kind(json!({"zoom": []})), Kind::UnknownName);
}

#[test]
fn mixed_units_order_uses_fps() {
    assert!(anim(json!({"x": [["1s", 0], [31, 1]]})).is_ok());
    assert!(anim(json!({"x": [["1s", 0], [29, 1]]})).is_err());
}

fn scene(layer: &str) -> Result<crate::Document> {
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
        r#"layers[0].anim.opacity: `opacity` is already animated by `in` (preset `fade-up`); expected `a in=none anim={"opacity":"0 -> 1 @0-12f ease-out","y":"580 -> 540 @0-12f ease-out"}` (the entrance as anim tracks; then adjust `opacity`), or define a scene preset without `opacity` under "presets""#
    );
}

#[test]
fn conflict_hint_uses_at_tuning_and_keeps_other_tracks() {
    let e = scene(
        r#""at":"100 760","in":"fade-up 60px 0.5s linear @15f","anim":{"opacity":"0 -> 1 @0-5f","rotate":"0 -> 5 @0-5f"}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(
        e.contains(r#"`a in=none anim={"opacity":"0 -> 1 @15-30f linear","y":"820 -> 760 @15-30f linear","rotate":"0 -> 5 @0-5f"}`"#),
        "{e}"
    );
    // The hint parses back and no longer conflicts.
    let fixed = scene(
        r#""at":"100 760","anim":{"opacity":"0 -> 1 @15-30f linear","y":"820 -> 760 @15-30f linear","rotate":"0 -> 5 @0-5f"}"#,
    );
    assert!(fixed.is_ok());
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
            let _ = s.parse::<TrackLine>();
            let _ = s.parse::<Span>();
        }

        #[test]
        fn string_form_shaped_input_never_panics(
            a in "[-0-9.a-z]{0,5}", b in "[-0-9.a-z]{0,5}", r in "[-@0-9.fs]{0,10}", c in "[a-z(),0-9.-]{0,12}"
        ) {
            let _ = format!("{a} -> {b} @{r} {c}").parse::<TrackLine>();
            let _ = anim(json!({"opacity": format!("{a} -> {b} @{r} {c}")}));
        }

        #[test]
        fn anim_never_panics(v in arb_json()) {
            let _ = anim(v.clone());
            let _ = anim(json!({"opacity": v.clone(), "x": v}));
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
