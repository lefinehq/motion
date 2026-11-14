//! Entrance presets as data: named start values for properties. The built-ins
//! are in `presets.json`; a scene's `presets` key adds or overrides them.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde_json::{Map, Value as Json};

use crate::doc::{AnimDoc, PresetsDoc};
use crate::fmt_num;
use crate::property::Target;
use crate::shorthand::Behaviour;
use crate::table;
use crate::validate;
use crate::value::Value;

const BUILTIN_JSON: &str = include_str!("presets.json");

/// The starts JSON writes as strings.
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
    /// The start `"+d"` or `"-d"` stands for.
    pub(crate) fn distance(text: &str) -> Option<Start> {
        table::lookup(&DISTANCES, text).map(|(_, start)| *start)
    }

    /// The accepted forms of a start: `a number, "+d" or "-d"`.
    pub(crate) fn form() -> &'static str {
        static FORM: OnceLock<String> = OnceLock::new();
        FORM.get_or_init(|| {
            let written: Vec<String> = table::names(&DISTANCES)
                .map(|name| format!("\"{name}\""))
                .collect();
            format!("a number, {}", written.join(" or "))
        })
    }

    /// The start value of a target resting at `rest`, for a behaviour that
    /// travels `distance` pixels. A number starts every component of a point;
    /// validation allows that only where the property broadcasts.
    pub fn resolve(self, rest: &Value, distance: f64) -> Value {
        let offset = |d: f64| Value::Number(rest.components()[0] + d);
        match self {
            Start::Value(v) => Value::splat(rest.value_type(), v),
            Start::Plus => offset(distance),
            Start::Minus => offset(-distance),
        }
    }
}

/// A preset: the properties or components it animates and where each starts.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetSpec {
    /// Each target and its start, in the order written.
    pub props: Vec<(Target, Start)>,
}

/// The built-in presets in file order, parsed once.
///
/// # Panics
/// If `presets.json` is invalid, which a unit test rules out.
pub fn builtin_presets() -> &'static [(String, PresetSpec)] {
    static BUILTIN: OnceLock<Vec<(String, PresetSpec)>> = OnceLock::new();
    BUILTIN.get_or_init(|| {
        let doc: PresetsDoc =
            serde_json::from_str(BUILTIN_JSON).expect("presets.json holds preset objects");
        validate::presets(&doc).expect("presets.json holds valid presets")
    })
}

/// A layer's entrance written as an `anim` object in JSON text, one string
/// track per property of `spec`, as the compiler builds it from `in`.
///
/// `rest` gives each target's resting value on the layer, or `None` when it
/// is not known yet; then there is no expansion. `others`, the layer's tracks
/// as written, are appended unless the preset animates them, so the result
/// can replace the whole `anim`.
pub(crate) fn expand_as_anim(
    spec: &PresetSpec,
    behaviour: &Behaviour,
    rest: impl Fn(Target) -> Option<Value>,
    fps: f64,
    others: &AnimDoc,
) -> Option<String> {
    let (start, end) = behaviour.frames(fps);
    let mut tracks: Map<String, Json> = spec
        .props
        .iter()
        .map(|&(target, from)| {
            let rest = rest(target)?;
            let from = from.resolve(&rest, behaviour.distance);
            let track = format!(
                "{from} -> {rest} @{}-{}f {}",
                fmt_num(start),
                fmt_num(end),
                behaviour.curve
            );
            Some((target.name().to_owned(), Json::String(track)))
        })
        .collect::<Option<_>>()?;
    for (name, track) in others {
        tracks.entry(name.as_str()).or_insert_with(|| {
            // A track holds only strings and finite numbers, which always convert.
            serde_json::to_value(track).expect("a track converts to JSON")
        });
    }
    Some(Json::Object(tracks).to_string())
}

/// Finds `name` among a scene's own presets first, then the built-ins.
pub(crate) fn lookup<'a>(own: &'a [(String, PresetSpec)], name: &str) -> Option<&'a PresetSpec> {
    table::lookup(own.iter().chain(builtin_presets()), name).map(|(_, spec)| spec)
}

/// The usable preset names, a scene's own first, without duplicates.
pub(crate) fn names(own: &[(String, PresetSpec)]) -> Vec<&str> {
    let mut seen = HashSet::new();
    table::names(own.iter().chain(builtin_presets()))
        .filter(|name| seen.insert(*name))
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/preset.rs"]
mod tests;
