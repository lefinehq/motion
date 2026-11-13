//! Entrance presets as data.
//!
//! Part of step 1 (Read). A preset is a named list of start values for
//! properties; the built-ins are in `presets.json` and a scene can add or
//! override presets with its `presets` key. Nothing here knows a preset name:
//! lookups and error messages read the tables.

use std::sync::OnceLock;

use serde_json::{Map, Value};

use crate::doc::{AnimDoc, PresetsDoc};
use crate::fmt_num;
use crate::property::{PROPERTIES, names as prop};
use crate::shorthand::{Behaviour, Coord, Position};
use crate::validate;

/// The built-in presets, embedded at compile time.
const BUILTIN_JSON: &str = include_str!("presets.json");

/// The starts JSON writes as strings, the one place they are spelled.
const DISTANCES: [(&str, Start); 2] = [("+d", Start::Plus), ("-d", Start::Minus)];

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

impl Start {
    /// The start a string stands for: `"+d"` or `"-d"`.
    pub(crate) fn distance(text: &str) -> Option<Start> {
        DISTANCES
            .iter()
            .find(|(written, _)| *written == text)
            .map(|(_, start)| *start)
    }
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
        let doc: PresetsDoc =
            serde_json::from_str(BUILTIN_JSON).expect("presets.json holds preset objects");
        validate::presets(&doc).expect("presets.json holds valid presets")
    })
}

/// Writes a layer's entrance as an `anim` object in JSON text: one string
/// track per property of `spec`, e.g.
/// `{"opacity":"0 -> 1 @15-27f ease-out","y":"760 -> 700 @15-27f ease-out"}`.
///
/// Each track runs from the preset's start value to the resting value, over
/// the behaviour's frames and with its curve, which is what the compiler
/// builds from `in`. The resting value is the registry default, except for
/// the two properties `at` sets, which are resolved against `canvas`
/// (`center` is half the size). `others` are the layer's `anim` tracks as
/// written; those for properties the preset does not animate are appended
/// unchanged, so the object can replace the whole `anim`.
pub(crate) fn expand_as_anim(
    spec: &PresetSpec,
    behaviour: &Behaviour,
    at: &Position,
    canvas: (f64, f64),
    fps: f64,
    others: &AnimDoc,
) -> String {
    let resolve = |c: Coord, extent: f64| match c {
        Coord::Center => extent / 2.0,
        Coord::Px(v) => v,
    };
    let start = behaviour.start.to_frames(fps);
    let end = start + behaviour.duration.to_frames(fps);
    let mut tracks = Map::new();
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
        tracks.insert(property.name.to_owned(), Value::String(track));
    }
    for (name, track) in others {
        tracks.entry(name.as_str()).or_insert_with(|| {
            // A track holds only strings and finite numbers, which always
            // convert.
            serde_json::to_value(track).expect("a track converts to JSON")
        });
    }
    Value::Object(tracks).to_string()
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
