//! Parsing and validating a scene document.
//!
//! Part of step 1 (Read). The JSON is converted by hand (not with serde
//! derive) so that every error can carry the path of the key that caused it.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::anim::{AnimTrack, parse_anim};
use crate::error::MUST_BE_STRING;
use crate::preset::{PresetSpec, lookup, names, parse_presets};
use crate::property::PROPERTIES;
use crate::shorthand::{
    BEHAVIOUR_EXPECTED, Behaviour, Coord, FONT_EXPECTED, Font, POSITION_EXPECTED, Position,
    RECT_EXPECTED, RectShape,
};
use crate::units::{COLOR_EXPECTED, Color, SIZE_EXPECTED, Size, TIME_EXPECTED, Time};
use crate::{Content, Document, FormatError, Layer, Scene};

/// The keys allowed at the top level of a scene.
pub const SCENE_KEYS: [&str; 7] = ["motion", "size", "fps", "dur", "bg", "presets", "layers"];
/// The keys allowed on a layer.
pub const LAYER_KEYS: [&str; 8] = ["id", "text", "rect", "font", "fill", "at", "in", "anim"];

/// Examples shown when a required key is missing or has the wrong type.
const SCENE_EXAMPLE: &str = r#"e.g. {"motion":1,"dur":"5s","layers":[]}"#;
const LAYER_EXAMPLE: &str = r#"e.g. {"id":"box","rect":"100x100"}"#;
const LAYERS_EXAMPLE: &str = r#"e.g. "layers":[{"id":"box","rect":"100x100"}]"#;
const TEXT_EXAMPLE: &str = r#"e.g. "text":"Hello""#;

pub(crate) const ID_RULE: &str =
    "ids use lowercase letters, digits, `-` and `_`, starting with a letter";

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

    let presets = match obj.get("presets") {
        None => Vec::new(),
        Some(v) => parse_presets(v, "presets")?,
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
        let layer = layer_from_value(item, i, &presets, fps)?;
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
        presets,
        layers,
    })
}

/// Converts the layer at index `i` of `layers`.
fn layer_from_value(
    value: &Value,
    i: usize,
    presets: &[(String, PresetSpec)],
    fps: f64,
) -> Result<Layer, FormatError> {
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
            let b = Behaviour::parse(s).map_err(|e| FormatError::from_shorthand(path("in"), e))?;
            if lookup(presets, &b.preset).is_none() {
                return Err(
                    FormatError::new(path("in"), format!("unknown preset `{}`", b.preset))
                        .expecting(format!("one of {}", names(presets).join(", "))),
                );
            }
            Some(b)
        }
        None => None,
    };

    let anim = match obj.get("anim") {
        None => Vec::new(),
        Some(v) => parse_anim(v, &path("anim"), fps)?,
    };
    if let Some(b) = &enter {
        check_conflicts(&anim, b, presets, &path("anim"))?;
    }

    Ok(Layer {
        id: id.to_string(),
        content,
        fill,
        at,
        enter,
        anim,
    })
}

/// Rejects an `anim` track for a property the `in` preset already animates:
/// two writers of one property would fight, so the author must pick one.
fn check_conflicts(
    anim: &[AnimTrack],
    behaviour: &Behaviour,
    presets: &[(String, PresetSpec)],
    path: &str,
) -> Result<(), FormatError> {
    let Some(spec) = lookup(presets, &behaviour.preset) else {
        return Ok(());
    };
    for track in anim {
        if spec.props.iter().any(|(p, _)| *p == track.property) {
            let name = PROPERTIES[track.property].name;
            return Err(FormatError::new(
                format!("{path}.{name}"),
                format!(
                    "`{name}` is already animated by `in` (preset `{}`)",
                    behaviour.preset
                ),
            )
            .expecting("remove it from `in` or put the whole animation in `anim`"));
        }
    }
    Ok(())
}

/// `[a-z][a-z0-9_-]*`
pub(crate) fn is_valid_id(id: &str) -> bool {
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
#[path = "../tests/unit/parse.rs"]
mod tests;
