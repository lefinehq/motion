//! Parsing and validating a scene document.
//!
//! Part of step 1 (Read). The JSON is converted by hand (not with serde
//! derive) so that every error can carry the path of the key that caused it.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::error::MUST_BE_STRING;
use crate::shorthand::{
    BEHAVIOUR_EXPECTED, Behaviour, Coord, FONT_EXPECTED, Font, POSITION_EXPECTED, Position,
    RECT_EXPECTED, RectShape,
};
use crate::units::{COLOR_EXPECTED, Color, SIZE_EXPECTED, Size, TIME_EXPECTED, Time};
use crate::{Content, Document, FormatError, Layer, Scene};

/// The keys allowed at the top level of a scene.
pub const SCENE_KEYS: [&str; 6] = ["motion", "size", "fps", "dur", "bg", "layers"];
/// The keys allowed on a layer.
pub const LAYER_KEYS: [&str; 7] = ["id", "text", "rect", "font", "fill", "at", "in"];

/// Examples shown when a required key is missing or has the wrong type.
const SCENE_EXAMPLE: &str = r#"e.g. {"motion":1,"dur":"5s","layers":[]}"#;
const LAYER_EXAMPLE: &str = r#"e.g. {"id":"box","rect":"100x100"}"#;
const LAYERS_EXAMPLE: &str = r#"e.g. "layers":[{"id":"box","rect":"100x100"}]"#;
const TEXT_EXAMPLE: &str = r#"e.g. "text":"Hello""#;

const ID_RULE: &str = "ids use lowercase letters, digits, `-` and `_`, starting with a letter";

/// Parses scene JSON text into a [`Document`]: the original value (kept so
/// edits and writes preserve what the author wrote) plus the typed scene.
pub fn parse_document(json: &str) -> Result<Document, FormatError> {
    let raw: Value = serde_json::from_str(json).map_err(|e| {
        FormatError::new("scene", format!("invalid JSON: {e}")).expecting(SCENE_EXAMPLE)
    })?;
    let scene = scene_from_value(&raw)?;
    Ok(Document { raw, scene })
}

/// Validates a JSON value and converts it into a typed [`Scene`], applying
/// defaults for the keys that may be left out.
pub fn scene_from_value(value: &Value) -> Result<Scene, FormatError> {
    let obj = value
        .as_object()
        .ok_or_else(|| FormatError::new("scene", "must be an object").expecting(SCENE_EXAMPLE))?;
    check_keys(obj, &SCENE_KEYS, "scene")?;

    match obj.get("motion") {
        None => return Err(missing("scene", "motion", r#""motion":1"#)),
        Some(v) if v.as_u64() == Some(1) => {}
        Some(v) => {
            return Err(
                FormatError::new("motion", format!("unsupported version {v}")).expecting("`1`"),
            );
        }
    }

    let size = match opt_str(obj, "size", "", SIZE_EXPECTED)? {
        Some(s) => Size::parse(s).map_err(|e| FormatError::from_shorthand("size", e))?,
        None => Size {
            width: 1920,
            height: 1080,
        },
    };
    let fps = match obj.get("fps") {
        None => 30.0,
        Some(v) => match v.as_f64() {
            Some(f) if f > 0.0 => f,
            _ => {
                return Err(
                    FormatError::new("fps", "must be a number above zero").expecting("e.g. `30`")
                );
            }
        },
    };
    let duration = match opt_str(obj, "dur", "", TIME_EXPECTED)? {
        Some(s) => Time::parse(s).map_err(|e| FormatError::from_shorthand("dur", e))?,
        None => return Err(missing("scene", "dur", r#""dur":"5s""#)),
    };
    let background = match opt_str(obj, "bg", "", COLOR_EXPECTED)? {
        Some(s) => Color::parse(s).map_err(|e| FormatError::from_shorthand("bg", e))?,
        None => Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    };

    let list = match obj.get("layers") {
        None => return Err(missing("scene", "layers", r#""layers":[]"#)),
        Some(Value::Array(a)) => a,
        Some(_) => {
            return Err(FormatError::new("layers", "must be an array").expecting(LAYERS_EXAMPLE));
        }
    };
    let mut seen = HashSet::new();
    let mut layers = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let layer = layer_from_value(item, i)?;
        if !seen.insert(layer.id.clone()) {
            return Err(FormatError::new(
                format!("layers[{i}].id"),
                format!("duplicate id `{}`", layer.id),
            )
            .expecting("every layer to have its own id"));
        }
        layers.push(layer);
    }

    Ok(Scene {
        version: 1,
        size,
        fps,
        duration,
        background,
        layers,
    })
}

/// Converts the layer at index `i` of `layers`.
fn layer_from_value(value: &Value, i: usize) -> Result<Layer, FormatError> {
    let base = format!("layers[{i}]");
    let obj = value
        .as_object()
        .ok_or_else(|| FormatError::new(&base, "must be an object").expecting(LAYER_EXAMPLE))?;
    check_keys(obj, &LAYER_KEYS, &base)?;

    let id = opt_str(obj, "id", &base, ID_RULE)?
        .ok_or_else(|| missing(&base, "id", r#""id":"title""#))?;
    if !is_valid_id(id) {
        return Err(
            FormatError::new(format!("{base}.id"), format!("invalid id `{id}`")).expecting(ID_RULE),
        );
    }

    let path = |key: &str| format!("{base}.{key}");
    let content = match (
        opt_str(obj, "text", &base, TEXT_EXAMPLE)?,
        opt_str(obj, "rect", &base, RECT_EXPECTED)?,
    ) {
        (Some(text), None) => {
            let font = match opt_str(obj, "font", &base, FONT_EXPECTED)? {
                Some(s) => {
                    Font::parse(s).map_err(|e| FormatError::from_shorthand(path("font"), e))?
                }
                None => Font {
                    family: "Inter".into(),
                    size: 48.0,
                    weight: 400,
                },
            };
            Content::Text {
                text: text.to_string(),
                font,
            }
        }
        (None, Some(rect)) => {
            if obj.contains_key("font") {
                return Err(
                    FormatError::new(path("font"), "`font` only applies to text layers")
                        .expecting("`font` only next to `text`"),
                );
            }
            Content::Rect(
                RectShape::parse(rect).map_err(|e| FormatError::from_shorthand(path("rect"), e))?,
            )
        }
        _ => {
            return Err(
                FormatError::new(&base, "needs exactly one of `text` or `rect`")
                    .expecting(r#"e.g. "text":"Hello" or "rect":"300x100""#),
            );
        }
    };

    let fill = match opt_str(obj, "fill", &base, COLOR_EXPECTED)? {
        Some(s) => Color::parse(s).map_err(|e| FormatError::from_shorthand(path("fill"), e))?,
        None => Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    };
    let at = match opt_str(obj, "at", &base, POSITION_EXPECTED)? {
        Some(s) => Position::parse(s).map_err(|e| FormatError::from_shorthand(path("at"), e))?,
        None => Position {
            x: Coord::Center,
            y: Coord::Center,
        },
    };
    let enter = match opt_str(obj, "in", &base, BEHAVIOUR_EXPECTED)? {
        Some(s) => {
            Some(Behaviour::parse(s).map_err(|e| FormatError::from_shorthand(path("in"), e))?)
        }
        None => None,
    };

    Ok(Layer {
        id: id.to_string(),
        content,
        fill,
        at,
        enter,
    })
}

/// `[a-z][a-z0-9_-]*`
fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Rejects any key not in `allowed`, listing the allowed ones.
fn check_keys(obj: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<(), FormatError> {
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        None => Ok(()),
        Some(k) => {
            let list: Vec<String> = allowed.iter().map(|a| format!("`{a}`")).collect();
            Err(FormatError::new(path, format!("unknown key `{k}`"))
                .expecting(format!("one of {}", list.join(", "))))
        }
    }
}

/// A required key that is absent; `example` shows it written out.
fn missing(path: &str, key: &str, example: &str) -> FormatError {
    FormatError::new(path, format!("missing `{key}`")).expecting(format!("e.g. {example}"))
}

/// Reads an optional string key. `parent` is the path of the object holding
/// it (empty at the top level, where a key's path is just its name).
/// `expected` is the accepted form, shown if the value is not a string.
fn opt_str<'a>(
    obj: &'a Map<String, Value>,
    key: &str,
    parent: &str,
    expected: &str,
) -> Result<Option<&'a str>, FormatError> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => {
            let path = if parent.is_empty() {
                key.to_string()
            } else {
                format!("{parent}.{key}")
            };
            Err(FormatError::new(path, MUST_BE_STRING).expecting(expected))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shorthand::{Coord, Position};
    use crate::units::{Color, Size};

    #[test]
    fn parses_minimal_scene_with_defaults() {
        let d = parse_document(r#"{"motion":1,"dur":"2s","layers":[{"id":"a","rect":"10x10"}]}"#)
            .unwrap();
        assert_eq!(d.scene.fps, 30.0);
        assert_eq!(
            d.scene.size,
            Size {
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            d.scene.background,
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            d.scene.layers[0].fill,
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            }
        );
        assert_eq!(
            d.scene.layers[0].at,
            Position {
                x: Coord::Center,
                y: Coord::Center
            }
        );
        assert!(d.scene.layers[0].enter.is_none());
    }

    #[test]
    fn text_layer_gets_the_default_font() {
        let d =
            parse_document(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":"Hi"}]}"#).unwrap();
        match &d.scene.layers[0].content {
            Content::Text { text, font } => {
                assert_eq!(text, "Hi");
                assert_eq!(font.family, "Inter");
                assert_eq!(font.size, 48.0);
                assert_eq!(font.weight, 400);
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn errors_carry_a_path_and_the_expected_form() {
        let e = parse_document(
            r#"{"motion":1,"dur":"2s","layers":[{"id":"a","rect":"10x10"},{"id":"b","rect":"10x10","in":"zoom"}]}"#,
        )
        .unwrap_err();
        assert_eq!(e.path, "layers[1].in");
        assert!(e.expected.is_some());
        assert!(
            e.to_string()
                .starts_with("layers[1].in: unknown preset `zoom`")
        );
    }

    #[test]
    fn rejects_structural_mistakes() {
        let err = |j: &str| parse_document(j).unwrap_err().to_string();
        assert!(err(r#"{"motion":2,"dur":"1s","layers":[]}"#).contains("unsupported version 2"));
        assert!(err(r#"{"motion":1,"layers":[]}"#).contains("scene: missing `dur`"));
        assert!(
            err(r##"{"motion":1,"dur":"1s","layers":[],"colour":"#FFF"}"##)
                .contains("unknown key `colour`")
        );
        assert!(
            err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a"}]}"#)
                .contains("needs exactly one of `text` or `rect`")
        );
        assert!(
            err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}]}"#)
                .contains("duplicate id `a`")
        );
        assert!(
            err(r#"{"motion":1,"dur":"1s","layers":[{"id":"Bad Id","rect":"1x1"}]}"#)
                .contains("ids use lowercase letters, digits, `-` and `_`, starting with a letter")
        );
        assert!(
            err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","font":"Inter 10"}]}"#)
                .contains("`font` only applies to text layers")
        );
        assert!(err("not json").starts_with("scene: invalid JSON"));
    }

    #[test]
    fn unknown_key_lists_the_allowed_keys() {
        let e = parse_document(r#"{"motion":1,"dur":"1s","layers":[],"colour":"x"}"#).unwrap_err();
        assert_eq!(e.path, "scene");
        assert!(e.to_string().contains("expected one of `motion`, `size`"));
        let e =
            parse_document(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","x":1}]}"#)
                .unwrap_err();
        assert_eq!(e.path, "layers[0]");
    }

    #[test]
    fn rejects_wrong_types_and_missing_required_keys() {
        let err = |j: &str| parse_document(j).unwrap_err();
        assert!(
            err("[]")
                .to_string()
                .starts_with("scene: must be an object; expected e.g. ")
        );
        assert!(
            err(r#"{"dur":"1s","layers":[]}"#)
                .to_string()
                .contains("missing `motion`")
        );
        assert!(
            err(r#"{"motion":1,"dur":"1s"}"#)
                .to_string()
                .contains("missing `layers`")
        );
        assert_eq!(err(r#"{"motion":1,"dur":5,"layers":[]}"#).path, "dur");
        assert_eq!(err(r#"{"motion":1,"dur":"1s","layers":{}}"#).path, "layers");
        assert_eq!(
            err(r#"{"motion":1,"dur":"1s","layers":[1]}"#).path,
            "layers[0]"
        );
        assert_eq!(
            err(r#"{"motion":1,"dur":"1s","layers":[{"rect":"1x1"}]}"#).path,
            "layers[0]"
        );
        assert_eq!(
            err(r#"{"motion":1,"dur":"1s","fps":0,"layers":[]}"#).path,
            "fps"
        );
        assert_eq!(
            err(r#"{"motion":1,"dur":"1s","fps":"x","layers":[]}"#).path,
            "fps"
        );
        assert_eq!(
            err(r#"{"motion":1,"dur":"1s","size":"huge","layers":[]}"#).path,
            "size"
        );
        assert_eq!(
            err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":3}]}"#).path,
            "layers[0].text"
        );
        assert_eq!(
            err(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","text":"x"}]}"#).path,
            "layers[0]"
        );
    }

    #[test]
    fn shorthand_failures_keep_their_expected_form() {
        let e = parse_document(r#"{"motion":1,"dur":"soon","layers":[]}"#).unwrap_err();
        assert_eq!(e.path, "dur");
        assert_eq!(
            e.to_string(),
            "dur: `soon` is not a valid time; expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`"
        );
    }

    /// The full error text of `json`.
    fn msg(json: &str) -> String {
        parse_document(json).unwrap_err().to_string()
    }

    #[test]
    fn missing_keys_show_an_example() {
        assert_eq!(
            msg(r#"{"motion":1,"layers":[]}"#),
            r#"scene: missing `dur`; expected e.g. "dur":"5s""#
        );
        assert_eq!(
            msg(r#"{"dur":"1s","layers":[]}"#),
            r#"scene: missing `motion`; expected e.g. "motion":1"#
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s"}"#),
            r#"scene: missing `layers`; expected e.g. "layers":[]"#
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[{"rect":"1x1"}]}"#),
            r#"layers[0]: missing `id`; expected e.g. "id":"title""#
        );
    }

    #[test]
    fn wrong_types_show_the_accepted_form_without_naming_the_key_twice() {
        let layer = |kv: &str| {
            msg(&format!(
                r#"{{"motion":1,"dur":"1s","layers":[{{"id":"a","rect":"1x1",{kv}}}]}}"#
            ))
        };
        assert_eq!(
            msg(r#"{"motion":1,"dur":5,"layers":[]}"#),
            "dur: must be a string; expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`"
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","size":3,"layers":[]}"#),
            "size: must be a string; expected `720p`, `1080p`, `4k`, `square`, `vertical` or `<width>x<height>`"
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","bg":0,"layers":[]}"#),
            "bg: must be a string; expected `#RGB`, `#RRGGBB` or `#RRGGBBAA`"
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","fps":"x","layers":[]}"#),
            "fps: must be a number above zero; expected e.g. `30`"
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":{}}"#),
            r#"layers: must be an array; expected e.g. "layers":[{"id":"box","rect":"100x100"}]"#
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[1]}"#),
            r#"layers[0]: must be an object; expected e.g. {"id":"box","rect":"100x100"}"#
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[{"id":3,"rect":"1x1"}]}"#),
            "layers[0].id: must be a string; expected ids use lowercase letters, digits, `-` and `_`, starting with a letter"
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":3}]}"#),
            r#"layers[0].text: must be a string; expected e.g. "text":"Hello""#
        );
        assert_eq!(
            layer(r#""fill":1"#),
            "layers[0].fill: must be a string; expected `#RGB`, `#RRGGBB` or `#RRGGBBAA`"
        );
        assert_eq!(
            layer(r#""at":1"#),
            "layers[0].at: must be a string; expected `<x> <y>`, each a number of pixels or `center`, e.g. `center 260`"
        );
        assert!(
            layer(r#""in":true"#).starts_with("layers[0].in: must be a string; expected `<preset>")
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":4}]}"#),
            "layers[0].rect: must be a string; expected `<width>x<height> [r<radius>]`, e.g. `300x400 r24`"
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","text":"x","font":9}]}"#),
            "layers[0].font: must be a string; expected `<family> <size> [<weight>]`, e.g. `Inter 96 700`"
        );
    }

    #[test]
    fn other_structural_errors_show_the_expected_form() {
        assert_eq!(
            msg("[]"),
            r#"scene: must be an object; expected e.g. {"motion":1,"dur":"5s","layers":[]}"#
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a"}]}"#),
            r#"layers[0]: needs exactly one of `text` or `rect`; expected e.g. "text":"Hello" or "rect":"300x100""#
        );
        assert_eq!(
            msg(r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1","font":"Inter 9"}]}"#),
            "layers[0].font: `font` only applies to text layers; expected `font` only next to `text`"
        );
        assert_eq!(
            msg(
                r#"{"motion":1,"dur":"1s","layers":[{"id":"a","rect":"1x1"},{"id":"a","rect":"1x1"}]}"#
            ),
            "layers[1].id: duplicate id `a`; expected every layer to have its own id"
        );
        assert!(msg("nope").starts_with("scene: invalid JSON: "));
        assert!(msg("nope").ends_with(r#"; expected e.g. {"motion":1,"dur":"5s","layers":[]}"#));
    }

    #[test]
    fn size_fps_and_bg_overrides_reach_the_scene() {
        let d = parse_document(
            r##"{"motion":1,"size":"720p","fps":60,"dur":"1s","bg":"#102030","layers":[]}"##,
        )
        .unwrap();
        assert_eq!(
            d.scene.size,
            Size {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(d.scene.fps, 60.0);
        assert_eq!(
            d.scene.background,
            Color {
                r: 0x10,
                g: 0x20,
                b: 0x30,
                a: 255
            }
        );
    }

    #[test]
    fn raw_keeps_the_original_value() {
        let d = parse_document(r#"{"layers":[],"dur":"1s","motion":1}"#).unwrap();
        assert_eq!(d.raw["dur"], "1s");
    }
}
