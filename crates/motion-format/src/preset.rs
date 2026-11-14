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
use crate::property::Prop;
use crate::shorthand::{Behaviour, Position};
use crate::table;
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
        table::lookup(&DISTANCES, text).map(|(_, start)| *start)
    }

    /// The accepted forms of a start, for errors: `a number, "+d" or "-d"`,
    /// with the strings read from the table.
    pub(crate) fn form() -> &'static str {
        static FORM: OnceLock<String> = OnceLock::new();
        FORM.get_or_init(|| {
            let written: Vec<String> = table::names(&DISTANCES)
                .map(|name| format!("\"{name}\""))
                .collect();
            format!("a number, {}", written.join(" or "))
        })
    }

    /// The value a property starts at, for a property resting at `rest`
    /// and a behaviour that travels `distance` pixels.
    pub fn resolve(self, rest: f64, distance: f64) -> f64 {
        match self {
            Start::Value(v) => v,
            Start::Plus => rest + distance,
            Start::Minus => rest - distance,
        }
    }
}

/// A preset: the properties it animates and where each starts.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetSpec {
    /// Each property and its start, in the order written.
    pub props: Vec<(Prop, Start)>,
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
/// builds from `in`. The resting value is the property's
/// [`rest`](crate::property::PropertySpec::rest): the registry default, or
/// the `at` axis resolved against `canvas`. `others` are the layer's `anim` tracks as
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
    let (start, end) = behaviour.frames(fps);
    let mut tracks = Map::new();
    for &(prop, from) in &spec.props {
        let rest = prop.spec().rest(at, canvas);
        let from = from.resolve(rest, behaviour.distance);
        let track = format!(
            "{} -> {} @{}-{}f {}",
            fmt_num(from),
            fmt_num(rest),
            fmt_num(start),
            fmt_num(end),
            behaviour.curve
        );
        tracks.insert(prop.name().to_owned(), Value::String(track));
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
    table::lookup(own.iter().chain(builtin_presets()), name).map(|(_, spec)| spec)
}

/// The names of the resolved table, a scene's own first, without duplicates.
pub(crate) fn names(own: &[(String, PresetSpec)]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for name in table::names(own.iter().chain(builtin_presets())) {
        // A scene's own preset hides the built-in of the same name.
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/preset.rs"]
mod tests;
