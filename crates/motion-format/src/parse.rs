//! Parsing and validating a scene document.
//!
//! Part of step 1 (Read). The JSON is converted by hand (not with serde
//! derive) so that every error can carry the path of the key that caused it.

use std::collections::HashSet;

use motion_diagnostic::{Diagnostic, Kind, Result};
use serde_json::{Map, Value};

use crate::anim::{AnimTrack, parse_anim};
use crate::preset::{PresetSpec, expand_as_anim, lookup, names, parse_presets};
use crate::property::PROPERTIES;
use crate::shorthand::{
    BEHAVIOUR_EXPECTED, Behaviour, Coord, FONT_EXPECTED, Font, POSITION_EXPECTED, Position,
    RECT_EXPECTED, RectShape,
};
use crate::units::{COLOR_EXPECTED, Color, SIZE_EXPECTED, Size, TIME_EXPECTED, Time};
use crate::{Content, Document, Layer, Scene};

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
pub fn parse_document(json: &str) -> Result<Document> {
    let raw: Value = serde_json::from_str(json).map_err(|e| {
        Diagnostic::syntax(format!("invalid JSON: {e}"))
            .at("scene")
            .expected(SCENE_EXAMPLE)
    })?;
    let scene = scene_from_value(&raw)?;
    Ok(Document { raw, scene })
}

/// Validates a JSON value and converts it into a typed [`Scene`], applying
/// defaults for the keys that may be left out.
pub fn scene_from_value(value: &Value) -> Result<Scene> {
    let obj = value.as_object().ok_or_else(|| {
        Diagnostic::new(Kind::WrongType, "must be an object")
            .at("scene")
            .expected(SCENE_EXAMPLE)
    })?;
    check_keys(obj, &SCENE_KEYS, "scene")?;

    match obj.get("motion") {
        None => return Err(missing("scene", "motion", r#""motion":1"#)),
        Some(v) if v.as_u64() == Some(1) => {}
        Some(v) => {
            return Err(
                Diagnostic::new(Kind::Invalid, format!("unsupported version {v}"))
                    .at("motion")
                    .expected("`1`"),
            );
        }
    }

    let size = match opt_str(obj, "size", "", SIZE_EXPECTED)? {
        Some(s) => Size::parse(s).map_err(|e| e.under("size"))?,
        None => Size {
            width: 1920,
            height: 1080,
        },
    };
    let fps = match obj.get("fps").map(Value::as_f64) {
        None => 30.0,
        Some(Some(f)) if f > 0.0 => f,
        // One message covers both failures; the kind tells a value that is
        // not a number from a number that is too small.
        Some(number) => {
            let kind = match number {
                Some(_) => Kind::OutOfRange,
                None => Kind::WrongType,
            };
            return Err(Diagnostic::new(kind, "must be a number above zero")
                .at("fps")
                .expected("e.g. `30`"));
        }
    };
    let duration = match opt_str(obj, "dur", "", TIME_EXPECTED)? {
        Some(s) => Time::parse(s).map_err(|e| e.under("dur"))?,
        None => return Err(missing("scene", "dur", r#""dur":"5s""#)),
    };
    let background = match opt_str(obj, "bg", "", COLOR_EXPECTED)? {
        Some(s) => Color::parse(s).map_err(|e| e.under("bg"))?,
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
            return Err(Diagnostic::new(Kind::WrongType, "must be an array")
                .at("layers")
                .expected(LAYERS_EXAMPLE));
        }
    };
    let mut seen = HashSet::new();
    let mut layers = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let layer = layer_from_value(item, i, &presets, fps, size)?;
        if !seen.insert(layer.id.clone()) {
            return Err(
                Diagnostic::new(Kind::Conflict, format!("duplicate id `{}`", layer.id))
                    .at(format!("layers[{i}].id"))
                    .expected("every layer to have its own id"),
            );
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
    size: Size,
) -> Result<Layer> {
    let base = format!("layers[{i}]");
    let obj = value.as_object().ok_or_else(|| {
        Diagnostic::new(Kind::WrongType, "must be an object")
            .at(&base)
            .expected(LAYER_EXAMPLE)
    })?;
    check_keys(obj, &LAYER_KEYS, &base)?;

    let id = opt_str(obj, "id", &base, ID_RULE)?
        .ok_or_else(|| missing(&base, "id", r#""id":"title""#))?;
    if !is_valid_id(id) {
        return Err(Diagnostic::new(Kind::Invalid, format!("invalid id `{id}`"))
            .at(format!("{base}.id"))
            .expected(ID_RULE));
    }

    let path = |key: &str| format!("{base}.{key}");
    let content = match (
        opt_str(obj, "text", &base, TEXT_EXAMPLE)?,
        opt_str(obj, "rect", &base, RECT_EXPECTED)?,
    ) {
        (Some(text), None) => {
            let font = match opt_str(obj, "font", &base, FONT_EXPECTED)? {
                Some(s) => Font::parse(s).map_err(|e| e.under(path("font")))?,
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
                    Diagnostic::new(Kind::Conflict, "`font` only applies to text layers")
                        .at(path("font"))
                        .expected("`font` only next to `text`"),
                );
            }
            Content::Rect(RectShape::parse(rect).map_err(|e| e.under(path("rect")))?)
        }
        // Neither is missing content; both are a contradiction. The text is
        // the same, since the fix is the same.
        (text, _) => {
            let kind = if text.is_none() {
                Kind::Missing
            } else {
                Kind::Conflict
            };
            return Err(
                Diagnostic::new(kind, "needs exactly one of `text` or `rect`")
                    .at(&base)
                    .expected(r#"e.g. "text":"Hello" or "rect":"300x100""#),
            );
        }
    };

    let fill = match opt_str(obj, "fill", &base, COLOR_EXPECTED)? {
        Some(s) => Color::parse(s).map_err(|e| e.under(path("fill")))?,
        None => Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    };
    let at = match opt_str(obj, "at", &base, POSITION_EXPECTED)? {
        Some(s) => Position::parse(s).map_err(|e| e.under(path("at")))?,
        None => Position {
            x: Coord::Center,
            y: Coord::Center,
        },
    };
    let enter = match opt_str(obj, "in", &base, BEHAVIOUR_EXPECTED)? {
        Some(s) => {
            let b = Behaviour::parse(s).map_err(|e| e.under(path("in")))?;
            if lookup(presets, &b.preset).is_none() {
                return Err(Diagnostic::unknown("preset", &b.preset, names(presets)).at(path("in")));
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
        let ctx = Conflict {
            id,
            behaviour: b,
            presets,
            at: &at,
            canvas: (f64::from(size.width), f64::from(size.height)),
            fps,
            raw_anim: obj.get("anim").and_then(Value::as_object),
        };
        check_conflicts(&anim, &ctx, &path("anim"))?;
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

/// What [`check_conflicts`] needs to say how to fix a conflict.
struct Conflict<'a> {
    /// The layer's id, so the suggested edit can be pasted as-is.
    id: &'a str,
    /// The layer's `in`.
    behaviour: &'a Behaviour,
    /// The scene's own presets.
    presets: &'a [(String, PresetSpec)],
    /// The layer's `at`.
    at: &'a Position,
    /// Canvas size in pixels.
    canvas: (f64, f64),
    /// Frames per second.
    fps: f64,
    /// The layer's `anim` as written.
    raw_anim: Option<&'a Map<String, Value>>,
}

/// Rejects an `anim` track for a property the `in` preset already animates:
/// two writers of one property would fight, so the author must pick one. The
/// error gives the exact edit that moves the entrance into `anim`.
fn check_conflicts(anim: &[AnimTrack], ctx: &Conflict, path: &str) -> Result<()> {
    let Some(spec) = lookup(ctx.presets, &ctx.behaviour.preset) else {
        return Ok(());
    };
    for track in anim {
        if spec.props.iter().any(|(p, _)| *p == track.property) {
            let name = PROPERTIES[track.property].name;
            let empty = Map::new();
            let expansion = expand_as_anim(
                spec,
                ctx.behaviour,
                ctx.at,
                ctx.canvas,
                ctx.fps,
                ctx.raw_anim.unwrap_or(&empty),
            );
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!(
                    "`{name}` is already animated by `in` (preset `{}`)",
                    ctx.behaviour.preset
                ),
            )
            .at(format!("{path}.{name}"))
            .expected(format!(
                "`{} in=none anim={expansion}` (the entrance as anim tracks; then adjust `{name}`), or define a scene preset without `{name}` under \"presets\"",
                ctx.id
            )));
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
fn check_keys(obj: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<()> {
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        None => Ok(()),
        Some(k) => Err(Diagnostic::unknown("key", k, allowed).at(path)),
    }
}

/// A required key that is absent; `example` shows it written out.
fn missing(path: &str, key: &str, example: &str) -> Diagnostic {
    Diagnostic::new(Kind::Missing, format!("missing `{key}`"))
        .at(path)
        .expected(format!("e.g. {example}"))
}

/// Reads an optional string key. `parent` is the path of the object holding
/// it (empty at the top level, where a key's path is just its name).
/// `expected` is the accepted form, shown if the value is not a string.
fn opt_str<'a>(
    obj: &'a Map<String, Value>,
    key: &str,
    parent: &str,
    expected: &'static str,
) -> Result<Option<&'a str>> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(Diagnostic::new(Kind::WrongType, "must be a string")
            .at(key)
            .under(parent)
            .expected(expected)),
    }
}

#[cfg(test)]
#[path = "../tests/unit/parse.rs"]
mod tests;
