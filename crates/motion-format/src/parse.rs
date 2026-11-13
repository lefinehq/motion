//! Parsing and validating a scene document.
//!
//! Part of step 1 (Read). The JSON is converted by hand (not with serde
//! derive) so that every error can carry the path of the key that caused it.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::shorthand::{Behaviour, Coord, Font, Position, RectShape};
use crate::units::{Color, Size, Time};
use crate::{Content, Document, FormatError, Layer, Scene};

/// Keys allowed at the top level of a scene, in canonical order.
pub const SCENE_KEYS: [&str; 6] = ["motion", "size", "fps", "dur", "bg", "layers"];
/// Keys allowed on a layer, in canonical order.
pub const LAYER_KEYS: [&str; 7] = ["id", "text", "rect", "font", "fill", "at", "in"];

const ID_RULE: &str = "ids use lowercase letters, digits, `-` and `_`, starting with a letter";

/// Parses scene JSON text into a [`Document`]: the original value (kept so
/// edits and writes preserve what the author wrote) plus the typed scene.
pub fn parse_document(json: &str) -> Result<Document, FormatError> {
    let raw: Value = serde_json::from_str(json)
        .map_err(|e| FormatError::new("scene", format!("invalid JSON: {e}")))?;
    let scene = scene_from_value(&raw)?;
    Ok(Document { raw, scene })
}

/// Validates a JSON value and converts it into a typed [`Scene`], applying
/// defaults for the keys that may be left out.
pub fn scene_from_value(value: &Value) -> Result<Scene, FormatError> {
    let obj = value
        .as_object()
        .ok_or_else(|| FormatError::new("scene", "expected an object"))?;
    check_keys(obj, &SCENE_KEYS, "scene")?;

    match obj.get("motion") {
        None => return Err(missing("scene", "motion")),
        Some(v) if v.as_u64() == Some(1) => {}
        Some(v) => {
            return Err(
                FormatError::new("motion", format!("unsupported version {v}")).expecting("`1`"),
            );
        }
    }

    let size = match opt_str(obj, "size", "")? {
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
                return Err(FormatError::new("fps", "fps must be a number above zero")
                    .expecting("e.g. `30`"));
            }
        },
    };
    let duration = match opt_str(obj, "dur", "")? {
        Some(s) => Time::parse(s).map_err(|e| FormatError::from_shorthand("dur", e))?,
        None => return Err(missing("scene", "dur")),
    };
    let background = match opt_str(obj, "bg", "")? {
        Some(s) => Color::parse(s).map_err(|e| FormatError::from_shorthand("bg", e))?,
        None => Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    };

    let list = match obj.get("layers") {
        None => return Err(missing("scene", "layers")),
        Some(Value::Array(a)) => a,
        Some(_) => return Err(FormatError::new("layers", "expected an array of layers")),
    };
    let mut seen = HashSet::new();
    let mut layers = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let layer = layer_from_value(item, i)?;
        if !seen.insert(layer.id.clone()) {
            return Err(FormatError::new(
                format!("layers[{i}].id"),
                format!("duplicate id `{}`", layer.id),
            ));
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
        .ok_or_else(|| FormatError::new(&base, "expected an object"))?;
    check_keys(obj, &LAYER_KEYS, &base)?;

    let id = opt_str(obj, "id", &base)?.ok_or_else(|| missing(&base, "id"))?;
    if !is_valid_id(id) {
        return Err(
            FormatError::new(format!("{base}.id"), format!("invalid id `{id}`")).expecting(ID_RULE),
        );
    }

    let path = |key: &str| format!("{base}.{key}");
    let content = match (opt_str(obj, "text", &base)?, opt_str(obj, "rect", &base)?) {
        (Some(text), None) => {
            let font = match opt_str(obj, "font", &base)? {
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
                return Err(FormatError::new(
                    path("font"),
                    "`font` only applies to text layers",
                ));
            }
            Content::Rect(
                RectShape::parse(rect).map_err(|e| FormatError::from_shorthand(path("rect"), e))?,
            )
        }
        _ => {
            return Err(FormatError::new(
                &base,
                "needs exactly one of `text` or `rect`",
            ));
        }
    };

    let fill = match opt_str(obj, "fill", &base)? {
        Some(s) => Color::parse(s).map_err(|e| FormatError::from_shorthand(path("fill"), e))?,
        None => Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    };
    let at = match opt_str(obj, "at", &base)? {
        Some(s) => Position::parse(s).map_err(|e| FormatError::from_shorthand(path("at"), e))?,
        None => Position {
            x: Coord::Center,
            y: Coord::Center,
        },
    };
    let enter = match opt_str(obj, "in", &base)? {
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

fn missing(path: &str, key: &str) -> FormatError {
    FormatError::new(path, format!("missing `{key}`"))
}

/// Reads an optional string key. `parent` is the path of the object holding
/// it (empty at the top level, where a key's path is just its name).
fn opt_str<'a>(
    obj: &'a Map<String, Value>,
    key: &str,
    parent: &str,
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
            Err(FormatError::new(path, format!("`{key}` must be a string")))
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
                .starts_with("scene: expected an object")
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

    #[test]
    fn raw_keeps_the_original_value() {
        let d = parse_document(r#"{"layers":[],"dur":"1s","motion":1}"#).unwrap();
        assert_eq!(d.raw["dur"], "1s");
    }
}
