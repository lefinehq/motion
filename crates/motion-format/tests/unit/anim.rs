//! Unit tests for `src/anim.rs`: the string grammar, and tracks read
//! through a whole scene.

use super::*;
use crate::Kind;
use crate::ValueType;
use crate::doc::{AnimDoc, Form};
use crate::parse::scene_from_value;
use crate::parse_document;
use crate::units::TIME_EXPECTED;
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

fn err(v: Value) -> Diagnostic {
    anim(v).unwrap_err()
}

/// The error of `text` written as the track `x`.
fn line_err(text: &str) -> Diagnostic {
    err(json!({ "x": text }))
}

/// Where the track `x` of [`anim`] sits.
const X: &str = "layers[1].anim.x";

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
        assert!(
            track
                .keys
                .iter()
                .all(|k| k.easing == Easing::All(Curve::Linear))
        );
    }
}

#[test]
fn bare_numbers_are_frames_and_order_follows_object() {
    let t = anim(json!({"y": [[2, 1]], "x": [[0, 1], [4.5, 2]]})).unwrap();
    assert_eq!(t[0].target.name(), "y");
    assert_eq!(t[1].target.name(), "x");
    assert_eq!(t[1].keys[1].time, Time::Frames(4.5));
}

#[test]
fn unknown_property_lists_names() {
    let e = err(json!({"glow": "0 -> 1 @0-5f"}));
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::UnknownName, Some("layers[1].anim.glow"))
    );
    let names: Vec<&str> = crate::property::target_names().collect();
    assert_eq!(
        e.expected,
        Some(format!("one of {}", names.join(", ")).into())
    );
}

#[test]
fn out_of_range_value() {
    let e = err(json!({"opacity": [[0, 0], [5, 1], [9, 2]]}));
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::OutOfRange, Some("layers[1].anim.opacity[2][1]"))
    );
    let e = err(json!({"opacity": "0 -> 2 @0-5f"}));
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::OutOfRange, Some("layers[1].anim.opacity"))
    );
}

#[test]
fn string_form_errors_give_the_form_of_the_part_that_failed() {
    for text in ["0 1 @0-5f", "0 -> 1", "0 -> 1 @0-5f ease ease"] {
        let e = line_err(text);
        assert_eq!(
            (e.kind, e.path.as_deref(), e.expected.as_deref()),
            (Kind::Syntax, Some(X), Some(STRING_EXPECTED)),
            "{text}"
        );
    }
    // A value is read as the target's type, and says that type's form.
    let e = line_err("a -> 1 @0-5f");
    assert_eq!(
        (e.kind, e.path.as_deref(), e.expected.as_deref()),
        (Kind::Syntax, Some(X), Some(ValueType::Number.form()))
    );
    // A failing range or curve reports its own diagnostic, placed at the track.
    for (text, part) in [
        ("0 -> 1 @0-5", TimeSpan::from_range("@0-5").map(drop)),
        ("0 -> 1 @05f", TimeSpan::from_range("@05f").map(drop)),
        ("0 -> 1 @0-5f bouncy", "bouncy".parse::<Curve>().map(drop)),
        (
            "0 -> 1 @0-5f cubic(1,2)",
            "cubic(1,2)".parse::<Curve>().map(drop),
        ),
    ] {
        assert_eq!(line_err(text), part.unwrap_err().at(X), "{text}");
    }
    let e = line_err("0 -> 1 @5f-0f");
    assert_eq!(
        (e.kind, e.path.as_deref(), e.expected.as_deref()),
        (Kind::Invalid, Some(X), Some(RANGE_EXPECTED))
    );
}

#[test]
fn string_form_problems_are_syntax_errors_with_a_form() {
    for text in ["0 1 @0-5f", "0 -> 1 2 @0-5f", "0 -> 1 @0-5f ease ease", ""] {
        let e = text.parse::<TrackLine>().unwrap_err();
        assert_eq!(
            (e.kind, e.path, e.expected.as_deref()),
            (Kind::Syntax, None, Some(STRING_EXPECTED)),
            "{text:?}"
        );
    }
    for text in ["0 -> 1 @05f", "0 -> 1 @0-5", "0 -> 1 @x-5f"] {
        let e = text.parse::<TrackLine>().unwrap_err();
        assert_eq!(
            (e.kind, e.expected.as_deref()),
            (Kind::Syntax, Some(RANGE_EXPECTED)),
            "{text}"
        );
    }
}

#[test]
fn ranges_borrow_the_end_unit() {
    let span = |text: &str| TimeSpan::from_range(text).unwrap();
    assert_eq!(
        span("@0-30f"),
        Span {
            start: Time::Frames(0.0),
            end: Time::Frames(30.0)
        }
    );
    assert_eq!(span("@1-1.5s").start, Time::Seconds(1.0));
    assert_eq!(span("@0f-1s").start, Time::Frames(0.0));
    assert!(TimeSpan::from_range("@-1-5f").is_err());
    // Without `@` it is a layer's span.
    assert_eq!("0-30f".parse::<TimeSpan>().unwrap(), span("@0-30f"));
    assert!("@0-30f".parse::<TimeSpan>().is_err());
}

#[test]
fn array_form_errors_say_what_kind_of_problem_they_are() {
    let x0 = format!("{X}[0]");
    let x1 = format!("{X}[1]");
    for (value, kind, path, form) in [
        (
            json!([1]),
            Kind::WrongType,
            "layers[1].anim",
            Some(AnimDoc::FORM),
        ),
        (json!({"x": 3}), Kind::WrongType, X, Some(TRACK_EXPECTED)),
        (json!({"x": []}), Kind::Missing, X, Some(KEYS_EXPECTED)),
        (json!({"x": [5]}), Kind::WrongType, &x0, Some(KEY_EXPECTED)),
        (json!({"x": [[0]]}), Kind::Invalid, &x0, Some(KEY_EXPECTED)),
        (
            json!({"x": [[0, 1, "linear", 4]]}),
            Kind::Invalid,
            &x0,
            Some(KEY_EXPECTED),
        ),
        (
            json!({"x": [[true, 0]]}),
            Kind::WrongType,
            &format!("{x0}[0]"),
            None,
        ),
        (
            json!({"x": [["12", 0]]}),
            Kind::Syntax,
            &x0,
            Some(TIME_EXPECTED),
        ),
        (json!({"x": [[-1, 0]]}), Kind::OutOfRange, &x0, None),
        (
            json!({"x": [[0, "a"]]}),
            Kind::Syntax,
            &format!("{x0}[1]"),
            Some(ValueType::Number.form()),
        ),
        (
            json!({"x": [{"t": 0, "v": [1, 2]}]}),
            Kind::WrongType,
            &format!("{x0}.v"),
            Some(ValueType::Number.form()),
        ),
        (
            json!({"x": [[0, 1, 3]]}),
            Kind::WrongType,
            &format!("{x0}[2]"),
            Some(EASING_EXPECTED),
        ),
        (
            json!({"x": [[0, 1, null]]}),
            Kind::WrongType,
            &format!("{x0}[2]"),
            Some(EASING_EXPECTED),
        ),
        (json!({"x": [[0, 1, "nope"]]}), Kind::UnknownName, &x0, None),
        (json!({"x": [[5, 0], [2, 1]]}), Kind::Invalid, &x1, None),
        (
            json!({"opacity": [[0, 2]]}),
            Kind::OutOfRange,
            "layers[1].anim.opacity[0][1]",
            None,
        ),
        (
            json!({"zoom": []}),
            Kind::UnknownName,
            "layers[1].anim.zoom",
            None,
        ),
    ] {
        let e = err(value.clone());
        assert_eq!((e.kind, e.path.as_deref()), (kind, Some(path)), "{value}");
        if let Some(form) = form {
            assert_eq!(e.expected.as_deref(), Some(form), "{value}");
        }
    }
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
    assert_eq!(d.scene().layers[0].anim.len(), 1);
    // A property the preset does not touch is fine next to `in`.
    assert!(scene(r#""in":"fade-up","anim":{"rotate":"0 -> 5 @0-5f"}"#).is_ok());
}

#[test]
fn conflict_with_in_preset() {
    let e = scene(r#""in":"fade-up","anim":{"opacity":"0 -> 1 @0-5f"}"#).unwrap_err();
    assert_eq!(
        (e.kind, e.path.as_deref()),
        (Kind::Conflict, Some("layers[0].anim.opacity"))
    );
    // The hint starts with the edit line that moves the entrance into `anim`.
    let hint = e.expected.unwrap();
    assert!(
        hint.starts_with(r#"`a in=none anim={"opacity":"0 -> 1 @0-12f ease-out","y":"580 -> 540 @0-12f ease-out"}`"#),
        "{hint}"
    );
}

#[test]
fn conflict_hint_uses_at_tuning_and_keeps_other_tracks() {
    let e = scene(
        r#""at":"100 760","in":"fade-up 60px 0.5s linear @15f","anim":{"opacity":"0 -> 1 @0-5f","rotate":"0 -> 5 @0-5f"}"#,
    )
    .unwrap_err();
    let hint = e.expected.unwrap();
    assert!(
        hint.starts_with(r#"`a in=none anim={"opacity":"0 -> 1 @15-30f linear","y":"820 -> 760 @15-30f linear","rotate":"0 -> 5 @0-5f"}`"#),
        "{hint}"
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
