//! Entrance presets as data.
//!
//! Part of step 1 (Read). A preset is a named list of start values for
//! properties; the built-ins are in `presets.json` and a scene can add or
//! override presets with its `presets` key. Nothing here knows a preset name:
//! lookups and error messages read the tables.

use std::sync::OnceLock;

use motion_diagnostic::{Diagnostic, Kind, Result};
use serde_json::{Map, Value};

use crate::anim::check_range;
use crate::fmt_num;
use crate::parse::{ID_RULE, is_valid_id};
use crate::property::{PROPERTIES, names as prop, property, property_names};
use crate::shorthand::{Behaviour, Coord, Position};

/// The built-in presets, embedded at compile time.
const BUILTIN_JSON: &str = include_str!("presets.json");

/// The accepted forms of a start value, for error messages.
const START_EXPECTED: &str = r#"a number, "+d" or "-d""#;

/// The JSON text of [`Start::Plus`].
const PLUS: &str = "+d";
/// The JSON text of [`Start::Minus`].
const MINUS: &str = "-d";

/// Where a property starts when an entrance begins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Start {
    /// An absolute value.
    Value(f64),
    /// The resting value plus the behaviour's distance; JSON `"+d"`.
    Plus,
    /// The resting value minus the behaviour's distance; JSON `"-d"`.
    Minus,
}

/// A preset: the properties it animates and where each starts.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetSpec {
    /// `(index into PROPERTIES, start)` in the order written.
    pub props: Vec<(usize, Start)>,
}

/// The built-in presets in file order, parsed once.
///
/// Panics if `presets.json` is invalid; a unit test guards the file.
pub fn builtin_presets() -> &'static [(String, PresetSpec)] {
    static BUILTIN: OnceLock<Vec<(String, PresetSpec)>> = OnceLock::new();
    BUILTIN.get_or_init(|| {
        let value: Value = serde_json::from_str(BUILTIN_JSON).expect("presets.json is valid JSON");
        parse_presets(&value, "presets.json").expect("presets.json holds valid presets")
    })
}

/// Parses a `presets` object: name to `{property: start}`. `path` prefixes
/// error paths (`presets` for a scene).
pub fn parse_presets(value: &Value, path: &str) -> Result<Vec<(String, PresetSpec)>> {
    let obj = value.as_object().ok_or_else(|| {
        Diagnostic::new(Kind::WrongType, "must be an object")
            .at(path)
            .expected(r#"e.g. {"slide":{"opacity":0,"x":"-d"}}"#)
    })?;
    let mut out = Vec::with_capacity(obj.len());
    for (name, body) in obj {
        let here = format!("{path}.{name}");
        if !is_valid_id(name) {
            return Err(
                Diagnostic::new(Kind::Invalid, format!("invalid preset name `{name}`"))
                    .at(&here)
                    .expected(ID_RULE),
            );
        }
        let props_obj = body.as_object().ok_or_else(|| {
            Diagnostic::new(Kind::WrongType, "must be an object of property starts")
                .at(&here)
                .expected(r#"e.g. {"opacity":0,"y":"+d"}"#)
        })?;
        let mut props = Vec::with_capacity(props_obj.len());
        for (prop, start) in props_obj {
            let at = format!("{here}.{prop}");
            let (index, spec) = property(prop)
                .ok_or_else(|| Diagnostic::unknown("property", prop, property_names()).at(&at))?;
            let start = parse_start(start, &at)?;
            match start {
                // An absolute start obeys the property's range, like an `anim` key.
                Start::Value(v) => {
                    check_range(v, spec, &at)?;
                }
                // A distance in pixels means nothing for a bounded property
                // such as opacity, and would leave its range.
                Start::Plus | Start::Minus if spec.range.is_some() => {
                    return Err(Diagnostic::new(
                        Kind::OutOfRange,
                        format!("`{prop}` has a fixed range, so it cannot start at a distance"),
                    )
                    .at(&at)
                    .expected("a number within its range, e.g. 0"));
                }
                Start::Plus | Start::Minus => {}
            }
            props.push((index, start));
        }
        if props.is_empty() {
            return Err(Diagnostic::new(
                Kind::Missing,
                format!("preset `{name}` sets no properties"),
            )
            .at(&here)
            .expected(r#"e.g. {"opacity":0}"#));
        }
        out.push((name.clone(), PresetSpec { props }));
    }
    Ok(out)
}

/// Reads one start value: a number, `"+d"` or `"-d"`.
fn parse_start(value: &Value, path: &str) -> Result<Start> {
    match value {
        Value::String(s) if s == PLUS => Ok(Start::Plus),
        Value::String(s) if s == MINUS => Ok(Start::Minus),
        // `as_f64` is `None` for anything but a number.
        _ => value
            .as_f64()
            .filter(|v| v.is_finite())
            .map(Start::Value)
            .ok_or_else(|| {
                // Any other string is the right type with the wrong content.
                let kind = if value.is_string() {
                    Kind::Invalid
                } else {
                    Kind::WrongType
                };
                Diagnostic::new(kind, format!("invalid start {value}"))
                    .at(path)
                    .expected(START_EXPECTED)
            }),
    }
}

/// Writes a layer's entrance as an `anim` object in JSON text: one string
/// track per property of `spec`, e.g.
/// `{"opacity":"0 -> 1 @15-27f ease-out","y":"760 -> 700 @15-27f ease-out"}`.
///
/// Each track runs from the preset's start value to the resting value, over
/// the behaviour's frames and with its curve, which is what the compiler
/// builds from `in`. The resting value is the registry default, except for
/// the two properties `at` sets, which are resolved against `canvas`
/// (`center` is half the size). `others` are the layer's existing `anim`
/// entries; those for properties the preset does not animate are appended
/// unchanged, so the object can replace the whole `anim`.
pub(crate) fn expand_as_anim(
    spec: &PresetSpec,
    behaviour: &Behaviour,
    at: &Position,
    canvas: (f64, f64),
    fps: f64,
    others: &Map<String, Value>,
) -> String {
    let resolve = |c: Coord, extent: f64| match c {
        Coord::Center => extent / 2.0,
        Coord::Px(v) => v,
    };
    let start = behaviour.start.to_frames(fps);
    let end = start + behaviour.duration.to_frames(fps);
    let mut entries: Vec<(&str, Value)> = Vec::new();
    for &(index, from) in &spec.props {
        let property = &PROPERTIES[index];
        let rest = match property.name {
            n if n == prop::X => resolve(at.x, canvas.0),
            n if n == prop::Y => resolve(at.y, canvas.1),
            _ => property.default,
        };
        let from = match from {
            Start::Value(v) => v,
            Start::Plus => rest + behaviour.distance,
            Start::Minus => rest - behaviour.distance,
        };
        let track = format!(
            "{} -> {} @{}-{}f {}",
            fmt_num(from),
            fmt_num(rest),
            fmt_num(start),
            fmt_num(end),
            behaviour.curve
        );
        entries.push((property.name, Value::String(track)));
    }
    for (name, value) in others {
        if !entries.iter().any(|(n, _)| n == name) {
            entries.push((name, value.clone()));
        }
    }
    let body: Vec<String> = entries
        .iter()
        .map(|(n, v)| format!("\"{n}\":{v}"))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// Finds `name` among a scene's own presets first, then the built-ins.
pub(crate) fn lookup<'a>(own: &'a [(String, PresetSpec)], name: &str) -> Option<&'a PresetSpec> {
    own.iter()
        .chain(builtin_presets())
        .find(|(n, _)| n == name)
        .map(|(_, spec)| spec)
}

/// The names of the resolved table, a scene's own first, without duplicates.
pub(crate) fn names(own: &[(String, PresetSpec)]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for (n, _) in own.iter().chain(builtin_presets()) {
        if !out.contains(&n.as_str()) {
            out.push(n);
        }
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/preset.rs"]
mod tests;
