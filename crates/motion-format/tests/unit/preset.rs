//! Unit tests for `src/preset.rs`.

use super::*;
use crate::parse_document;

fn presets(json: &str) -> Result<Vec<(String, PresetSpec)>, FormatError> {
    parse_presets(&serde_json::from_str(json).unwrap(), "presets")
}

#[test]
fn builtin_file_parses_and_names_known_properties() {
    let table = builtin_presets();
    assert!(!table.is_empty());
    for (name, spec) in table {
        assert!(is_valid_id(name), "{name}");
        for (i, _) in &spec.props {
            assert!(*i < crate::property::PROPERTIES.len(), "{name}");
        }
    }
    // Order of the file is kept.
    assert_eq!(table[0].0, "fade");
    let up = lookup(&[], "fade-up").unwrap();
    let y = property("y").unwrap().0;
    assert!(up.props.contains(&(y, Start::Plus)));
}

#[test]
fn start_forms_parse() {
    let t = presets(r#"{"a":{"x":"+d","y":"-d","rotate":-90,"scale":0.5}}"#).unwrap();
    let starts: Vec<Start> = t[0].1.props.iter().map(|(_, s)| *s).collect();
    assert_eq!(
        starts,
        [
            Start::Plus,
            Start::Minus,
            Start::Value(-90.0),
            Start::Value(0.5)
        ]
    );
}

#[test]
fn bad_start_value_shows_expected_form() {
    let e = presets(r#"{"a":{"x":"left"}}"#).unwrap_err();
    assert_eq!(e.path, "presets.a.x");
    assert!(
        e.to_string()
            .ends_with(r#"; expected a number, "+d" or "-d""#),
        "{e}"
    );
    assert!(presets(r#"{"a":{"x":true}}"#).is_err());
}

#[test]
fn unknown_property_lists_the_registry() {
    let e = presets(r#"{"a":{"zoom":1}}"#).unwrap_err();
    assert_eq!(e.path, "presets.a.zoom");
    assert!(e.to_string().contains("unknown property `zoom`"));
    assert!(
        e.to_string()
            .contains("expected one of x, y, opacity, scale, rotate")
    );
}

#[test]
fn bad_names_and_shapes_are_rejected() {
    assert!(
        presets(r#"{"Bad":{}}"#)
            .unwrap_err()
            .to_string()
            .contains("invalid preset name")
    );
    assert!(presets(r#"{"a":3}"#).is_err());
    assert!(parse_presets(&Value::Null, "presets").is_err());
}

const BASE: &str = r#""motion":1,"dur":"1s""#;

#[test]
fn scene_preset_adds_a_name_and_overrides_a_builtin() {
    let d = parse_document(&format!(
        r#"{{{BASE},"presets":{{"slide":{{"x":"-d"}},"fade":{{"opacity":0,"y":"+d"}}}},
        "layers":[{{"id":"a","rect":"1x1","in":"slide"}},{{"id":"b","rect":"1x1","in":"fade"}}]}}"#
    ))
    .unwrap();
    let s = &d.scene;
    assert_eq!(s.preset("slide").unwrap().props.len(), 1);
    assert_eq!(s.preset("fade").unwrap().props.len(), 2);
    assert_eq!(s.preset("pop").unwrap().props.len(), 2);
    assert!(s.preset("zoom").is_none());
}

#[test]
fn unknown_preset_lists_resolved_names_without_duplicates() {
    let e = parse_document(&format!(
        r#"{{{BASE},"presets":{{"slide":{{"x":1}},"fade":{{"y":1}}}},"layers":[{{"id":"a","rect":"1x1","in":"zoom"}}]}}"#
    ))
    .unwrap_err();
    assert_eq!(e.path, "layers[0].in");
    let text = e.to_string();
    assert!(
        text.starts_with(
            "layers[0].in: unknown preset `zoom`; expected one of slide, fade, fade-up"
        ),
        "{text}"
    );
    assert_eq!(text.matches("fade,").count(), 1, "{text}");
    assert!(text.contains("spin-in"));
}

mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_presets_any_json(s in ".*") {
            if let Ok(v) = serde_json::from_str::<Value>(&s) {
                let _ = parse_presets(&v, "presets");
            }
        }

        #[test]
        fn parse_presets_small_objects(
            name in "[ -~]{0,8}",
            prop in "[a-z]{0,8}",
            start in "[ -~]{0,4}",
            num in proptest::option::of(-1e6f64..1e6),
        ) {
            let v = match num {
                Some(n) => serde_json::json!({ name: { prop: n } }),
                None => serde_json::json!({ name: { prop: start } }),
            };
            let _ = parse_presets(&v, "presets");
        }
    }
}

#[test]
fn absolute_start_obeys_the_property_range() {
    let e = presets(r#"{"a":{"opacity":2}}"#).unwrap_err().to_string();
    assert_eq!(
        e,
        "presets.a.opacity: value 2 is outside 0..=1; expected a number from 0 to 1"
    );
    // Unbounded properties take any value.
    assert!(presets(r#"{"a":{"x":1900,"opacity":0.5}}"#).is_ok());
}

#[test]
fn empty_preset_is_rejected() {
    let e = presets(r#"{"x":{}}"#).unwrap_err().to_string();
    assert_eq!(
        e,
        r#"presets.x: preset `x` sets no properties; expected e.g. {"opacity":0}"#
    );
}

#[test]
fn expand_as_anim_writes_the_entrance_as_tracks() {
    use crate::shorthand::Position;
    let spec = presets(r#"{"p":{"opacity":0,"x":"-d","y":"+d","scale":0.5,"rotate":"+d"}}"#)
        .unwrap()
        .remove(0)
        .1;
    let b = Behaviour::parse("p 30px 1s cubic(.2,0,0,1) @10f").unwrap();
    let at = Position::parse("center 300").unwrap();
    let others: Map<String, Value> = serde_json::from_str(r#"{"opacity":"x","glow":1}"#).unwrap();
    let json = expand_as_anim(&spec, &b, &at, (1000.0, 500.0), 30.0, &others);
    assert_eq!(
        json,
        r#"{"opacity":"0 -> 1 @10-40f cubic(0.2,0,0,1)","x":"470 -> 500 @10-40f cubic(0.2,0,0,1)","y":"330 -> 300 @10-40f cubic(0.2,0,0,1)","scale":"0.5 -> 1 @10-40f cubic(0.2,0,0,1)","rotate":"30 -> 0 @10-40f cubic(0.2,0,0,1)","glow":1}"#
    );
    // The output is valid JSON.
    assert!(serde_json::from_str::<Value>(&json).is_ok());
}

#[test]
fn a_bounded_property_cannot_start_at_a_distance() {
    let e = parse_presets(&serde_json::json!({"dim":{"opacity":"-d"}}), "presets")
        .unwrap_err()
        .to_string();
    assert_eq!(
        e,
        "presets.dim.opacity: `opacity` has a fixed range, so it cannot start at a distance; expected a number within its range, e.g. 0"
    );
    // Unbounded properties still may.
    assert!(parse_presets(&serde_json::json!({"rise":{"y":"+d"}}), "presets").is_ok());
}
