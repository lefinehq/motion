//! An edited scene back into the project it was imported from ([D45]).
//!
//! 1. [`plan`] compares the edited scene with the scene the import gives
//!    for the original project, value by value. Layers and comps are
//!    matched by id, other array elements by index.
//! 2. Each changed value the import read from a stored static value is
//!    converted back to the project's units and encoded as the decoder
//!    reads it. Every other change is reported with why it is not written.
//! 3. [`apply`] patches those bytes into the original file; every other
//!    byte stays as it was.
//! 4. [`verify`] compares the scene imported from the patched file with
//!    the edited scene, at every value written.
//!
//! [D45]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d45-export-patches-the-project-it-came-from

use std::fmt;

use motion_format::{Diagnostic, Kind, Result, ValueType};
use serde::Serialize;
use serde_json::Value as Json;

use crate::map::Sources;
use crate::map::keys::Convert;
use crate::map::sources::{Source, Writable, step};
use crate::model::{Color, Origin, Value};
use crate::rifx::{self, Patch};
use motion_format::Step;
use motion_format::address::{json_at, key_text};

/// The step of the numbers the import writes: it rounds them to six
/// decimals. A value read back is compared with the edited one in steps.
const READ_BACK: f64 = 1e-6;

/// One change in the edited scene.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    /// Where it is, as `layers[title].opacity`.
    pub at: String,
    /// The value the original project gives.
    pub from: Option<Json>,
    /// The edited value.
    pub to: Option<Json>,
    /// Why it is not written back, for one that is not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// What an export writes, and what it cannot.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Plan {
    /// The changes written back.
    pub written: Vec<Change>,
    /// The changes not written back, each with why.
    pub not_written: Vec<Change>,
    /// The bytes to patch.
    #[serde(skip)]
    patches: Vec<Patch>,
    /// The paths written, each with the value the import should give there
    /// once patched, as the import itself writes it.
    #[serde(skip)]
    expected: Vec<(Vec<Step>, Json)>,
}

impl Plan {
    /// How many bytes of the file the plan changes.
    pub fn patched_bytes(&self) -> usize {
        self.patches.iter().map(|p| p.bytes.len()).sum()
    }
}

/// Plans the export of `edited`, a scene first imported from a project
/// whose import gives `original`, with `sources`.
pub fn plan(original: &Json, sources: &Sources, edited: &Json) -> Plan {
    let mut differences = Vec::new();
    diff(original, edited, &mut Vec::new(), &mut differences);
    let mut plan = Plan::default();
    let mut planned: Vec<&Source> = Vec::new();
    for difference in differences {
        let source = sources
            .0
            .iter()
            .filter(|s| difference.path.starts_with(&s.path))
            .max_by_key(|s| s.path.len());
        let why = match (&difference.kind, source.map(|s| &s.writable)) {
            (Difference::Structure, _) => {
                Some("the original has no such layer, item or key".to_owned())
            }
            (_, Some(Writable::Static { .. })) => None,
            (_, Some(Writable::Expression)) => Some(
                "an expression sets it in the project, so its stored value would change nothing"
                    .to_owned(),
            ),
            (_, Some(Writable::Keyed)) => {
                Some("it is keyed, and keys are not written back yet".to_owned())
            }
            (_, None) => Some(
                "the project stores no value for it that the export can write (a default, a preset, a setting, text, or a value it computes)"
                    .to_owned(),
            ),
        };
        match (why, source) {
            (None, Some(source)) => {
                if !planned.iter().any(|p| p.path == source.path) {
                    planned.push(source);
                }
            }
            (why, _) => plan.not_written.push(Change {
                at: key_text(&difference.path),
                from: json_at(original, &difference.path).cloned(),
                to: json_at(edited, &difference.path).cloned(),
                why,
            }),
        }
    }
    for source in planned {
        let Writable::Static {
            origin,
            convert,
            template,
        } = &source.writable
        else {
            continue;
        };
        let change = |why: Option<String>| Change {
            at: key_text(&source.path),
            from: json_at(original, &source.path).cloned(),
            to: json_at(edited, &source.path).cloned(),
            why,
        };
        let value = json_at(edited, &source.path)
            .ok_or_else(|| "the edited scene has no value there".to_owned())
            .and_then(|json| project_value(json, *convert, template));
        let encoded = value.and_then(|value| {
            let expected = convert
                .value(&value)
                .ok_or_else(|| "a value the import cannot write back".to_owned())?;
            Ok((encode(&value, origin)?, expected))
        });
        match encoded {
            Ok((bytes, expected)) => {
                plan.patches.push(Patch {
                    range: origin.numbers.clone(),
                    bytes,
                });
                plan.expected.push((source.path.clone(), expected));
                plan.written.push(change(None));
            }
            Err(why) => plan.not_written.push(change(Some(why))),
        }
    }
    plan
}

/// `file`, the project the original scene was imported from, with the
/// plan's patches.
///
/// # Errors
/// A diagnostic when the file is not a RIFX project or a patch does not fit
/// it: the file is not the one the plan was made from.
pub fn apply(file: &[u8], plan: &Plan) -> Result<Vec<u8>> {
    rifx::write(file, &plan.patches)
}

/// The values the plan wrote that `reimported`, the scene imported from the
/// patched file, does not give as the import writes the edited values.
pub fn verify(plan: &Plan, reimported: &Json) -> Vec<Change> {
    plan.expected
        .iter()
        .filter_map(|(path, expected)| {
            let got = json_at(reimported, path);
            (!got.is_some_and(|got| close(got, expected))).then(|| Change {
                at: key_text(path),
                from: got.cloned(),
                to: Some(expected.clone()),
                why: Some("the patched project does not read back as edited".into()),
            })
        })
        .collect()
}

/// Whether two values are the same, numbers to the precision the import
/// writes.
/// Whether two numbers the import wrote differ by at most one of its
/// steps (relative to their size above 1). Both are rounded from the same
/// value, one of them after a trip through the file's encoding, so a value
/// half a step from both neighbours can round either way.
fn within_a_step(x: f64, y: f64) -> bool {
    let step = READ_BACK * x.abs().max(y.abs()).max(1.0);
    // The slack absorbs the binary noise of the subtraction, far below a step.
    (x - y).abs() <= step * (1.0 + 1e-6)
}

fn close(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(x), Json::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => within_a_step(x, y),
            _ => x == y,
        },
        (Json::Array(x), Json::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| close(a, b))
        }
        (Json::Object(x), Json::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| close(v, w)))
        }
        _ => a == b,
    }
}

/// What differs at one path.
#[derive(Debug, Clone, PartialEq)]
enum Difference {
    /// A value changed.
    Value,
    /// A key, layer or item is there in one scene and not the other.
    Structure,
}

/// One place where the scenes differ.
#[derive(Debug, Clone, PartialEq)]
struct Found {
    path: Vec<Step>,
    kind: Difference,
}

/// Every place where `edited` differs from `original`, down to single
/// values.
fn diff(original: &Json, edited: &Json, path: &mut Vec<Step>, found: &mut Vec<Found>) {
    match (original, edited) {
        (Json::Object(a), Json::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                path.push(Step::Key(key.clone()));
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => diff(x, y, path, found),
                    _ => found.push(Found {
                        path: path.clone(),
                        kind: Difference::Structure,
                    }),
                }
                path.pop();
            }
        }
        (Json::Array(a), Json::Array(b)) => {
            let steps = |items: &[Json]| -> Vec<Step> {
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| step(item, i))
                    .collect()
            };
            let (left, right) = (steps(a), steps(b));
            let mut all: Vec<&Step> = left.iter().collect();
            all.extend(right.iter().filter(|s| !left.contains(s)));
            for s in all {
                path.push(s.clone());
                let get = |items: &'_ [Json], steps: &[Step]| {
                    steps.iter().position(|t| t == s).map(|i| items[i].clone())
                };
                match (get(a, &left), get(b, &right)) {
                    (Some(x), Some(y)) => diff(&x, &y, path, found),
                    _ => found.push(Found {
                        path: path.clone(),
                        kind: Difference::Structure,
                    }),
                }
                path.pop();
            }
        }
        (a, b) if close(a, b) => {}
        _ => found.push(Found {
            path: path.clone(),
            kind: Difference::Value,
        }),
    }
}

/// An edited scene value as the project's value: the inverse of the
/// import's conversion, keeping the project's other components.
fn project_value(
    json: &Json,
    convert: Convert,
    template: &Value,
) -> std::result::Result<Value, String> {
    let (ty, broadcast) = match convert {
        Convert::Number | Convert::Percent => (ValueType::Number, false),
        Convert::Point | Convert::PointPercent => (ValueType::Point, true),
        Convert::Color => (ValueType::Color, false),
        Convert::Path => return Err("a path, which the export does not write yet".into()),
    };
    let parsed = match json {
        Json::String(text) => motion_format::Value::parse(text, ty, broadcast),
        Json::Number(n) => {
            motion_format::Value::from_numbers(&[n.as_f64().unwrap_or(f64::NAN)], ty, broadcast)
        }
        Json::Array(items) => {
            let numbers: Vec<f64> = items
                .iter()
                .map(|n| n.as_f64().unwrap_or(f64::NAN))
                .collect();
            motion_format::Value::from_numbers(&numbers, ty, broadcast)
        }
        _ => return Err(format!("{json} is not a value of this kind")),
    }
    .map_err(|e| e.message)?;
    let factor = convert.factor();
    let numbers: Vec<f64> = parsed.components().iter().map(|n| n / factor).collect();
    if numbers.iter().any(|n| !n.is_finite()) {
        return Err("a value that is not a finite number".into());
    }
    Ok(match template {
        Value::Color(_) => match *numbers.as_slice() {
            [r, g, b, a] => Value::Color(Color { r, g, b, a }),
            [r, g, b] => Value::Color(Color { r, g, b, a: 1.0 }),
            _ => return Err("a colour of other than 3 or 4 channels".into()),
        },
        other => other.with_numbers(&numbers),
    })
}

/// `value` as the bytes at `origin`: big-endian doubles, a colour as alpha,
/// red, green and blue from 0 to 255.
fn encode(value: &Value, origin: &Origin) -> std::result::Result<Vec<u8>, String> {
    let numbers = match (value, origin.color) {
        (Value::Color(c), true) => vec![c.a * 255.0, c.r * 255.0, c.g * 255.0, c.b * 255.0],
        (value, false) => value
            .numbers()
            .ok_or_else(|| "a value that is not numbers".to_owned())?,
        _ => return Err("a value of another kind than the project stores".into()),
    };
    if numbers.len() < origin.dims {
        return Err(format!(
            "{} numbers where the project stores {}",
            numbers.len(),
            origin.dims
        ));
    }
    Ok(numbers[..origin.dims]
        .iter()
        .flat_map(|n| n.to_be_bytes())
        .collect())
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let json = |v: &Option<Json>| v.as_ref().map_or("nothing".into(), Json::to_string);
        writeln!(
            f,
            "  {} written back, {} not",
            self.written.len(),
            self.not_written.len()
        )?;
        for change in &self.written {
            writeln!(
                f,
                "    {}: {} -> {}",
                change.at,
                json(&change.from),
                json(&change.to)
            )?;
        }
        if !self.not_written.is_empty() {
            writeln!(f, "  not written back:")?;
        }
        for change in &self.not_written {
            writeln!(
                f,
                "    {}: {} -> {}: {}",
                change.at,
                json(&change.from),
                json(&change.to),
                change.why.as_deref().unwrap_or_default()
            )?;
        }
        Ok(())
    }
}

/// The diagnostic for a patched project that does not read back as edited.
pub fn mismatch(changes: &[Change]) -> Diagnostic {
    let listed: Vec<String> = changes
        .iter()
        .map(|c| {
            format!(
                "{} reads back as {}",
                c.at,
                c.from.as_ref().map_or("nothing".into(), Json::to_string)
            )
        })
        .collect();
    Diagnostic::new(
        Kind::Invalid,
        format!(
            "the patched project does not read back as edited, so it was not written: {}",
            listed.join("; ")
        ),
    )
}

#[cfg(test)]
#[path = "../tests/unit/export.rs"]
mod tests;
