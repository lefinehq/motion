//! Entrance presets as data.
//!
//! Part of step 1 (Read). A preset is a named list of start values for
//! properties; the built-ins are in `presets.json` and a scene can add or
//! override presets with its `presets` key. Nothing here knows a preset name:
//! lookups and error messages read the tables.

use std::sync::OnceLock;

use serde_json::Value;

use crate::FormatError;
use crate::parse::{ID_RULE, is_valid_id};
use crate::property::{property, property_names};

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
pub fn parse_presets(value: &Value, path: &str) -> Result<Vec<(String, PresetSpec)>, FormatError> {
    let obj = value.as_object().ok_or_else(|| {
        FormatError::new(path, "must be an object")
            .expecting(r#"e.g. {"slide":{"opacity":0,"x":"-d"}}"#)
    })?;
    let mut out = Vec::with_capacity(obj.len());
    for (name, body) in obj {
        let here = format!("{path}.{name}");
        if !is_valid_id(name) {
            return Err(
                FormatError::new(&here, format!("invalid preset name `{name}`")).expecting(ID_RULE),
            );
        }
        let props_obj = body.as_object().ok_or_else(|| {
            FormatError::new(&here, "must be an object of property starts")
                .expecting(r#"e.g. {"opacity":0,"y":"+d"}"#)
        })?;
        let mut props = Vec::with_capacity(props_obj.len());
        for (prop, start) in props_obj {
            let at = format!("{here}.{prop}");
            let (index, _) = property(prop).ok_or_else(|| {
                FormatError::new(&at, format!("unknown property `{prop}`"))
                    .expecting(format!("one of {}", property_names()))
            })?;
            props.push((index, parse_start(start, &at)?));
        }
        out.push((name.clone(), PresetSpec { props }));
    }
    Ok(out)
}

/// Reads one start value: a number, `"+d"` or `"-d"`.
fn parse_start(value: &Value, path: &str) -> Result<Start, FormatError> {
    match value {
        Value::Number(n) => n
            .as_f64()
            .filter(|v| v.is_finite())
            .map(Start::Value)
            .ok_or_else(|| {
                FormatError::new(path, format!("invalid start {value}")).expecting(START_EXPECTED)
            }),
        Value::String(s) if s == PLUS => Ok(Start::Plus),
        Value::String(s) if s == MINUS => Ok(Start::Minus),
        other => {
            Err(FormatError::new(path, format!("invalid start {other}")).expecting(START_EXPECTED))
        }
    }
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
