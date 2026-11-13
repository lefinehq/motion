//! Free-form animation tracks: the `anim` key of a layer.
//!
//! Part of step 1 (Read). `anim` maps a property name to either a one-line
//! string (`"0 -> 200 @0-30f ease-out"`) or a list of keyframes. Both forms
//! produce the same [`AnimTrack`]. Property names and ranges come from the
//! registry in [`crate::property`]; curve names from [`crate::Curve`].

use motion_diagnostic::{Diagnostic, Kind, Result};
use serde_json::Value;

use crate::property::{PropertySpec, property, property_names};
use crate::units::{TIME_EXPECTED, Time};
use crate::{Curve, fmt_num};

/// The accepted forms of a track, for error messages.
const TRACK_EXPECTED: &str = r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out", or a list of [time, value] keys"#;
/// The string form only, for errors that are about its pieces.
const STRING_EXPECTED: &str =
    r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out""#;
/// The accepted forms of a key list.
const KEYS_EXPECTED: &str = r#"a list of keys like [[0, 0], ["12f", 1, "ease-out"]]"#;
/// The accepted forms of a range such as `@0-30f`.
const RANGE_EXPECTED: &str = "`@<start>-<end>` with a unit, e.g. `@0-30f`, `@0.5s-1s`";
/// The arrow between the two values of the string form.
const ARROW: &str = "->";
/// The marker that starts the range token.
const RANGE_MARK: char = '@';

/// One keyframe of a track.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimKey {
    /// When the key sits, in the unit the author wrote.
    pub time: Time,
    /// The property's value at that time.
    pub value: f64,
    /// How the value eases from this key to the next; `linear` by default.
    pub curve: Curve,
}

/// The keys of one property.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimTrack {
    /// Index into [`PROPERTIES`](crate::property::PROPERTIES).
    pub property: usize,
    /// Keys in non-decreasing time order; never empty.
    pub keys: Vec<AnimKey>,
}

/// Parses the `anim` object found at `path` (for example `layers[1].anim`).
///
/// Tracks come back in the order the author wrote them. `fps` is needed only
/// to check that keys given in mixed units are in time order.
pub fn parse_anim(value: &Value, path: &str, fps: f64) -> Result<Vec<AnimTrack>> {
    let obj = value.as_object().ok_or_else(|| {
        Diagnostic::new(Kind::WrongType, "must be an object of property tracks")
            .at(path)
            .expected(r#"e.g. {"opacity":"0 -> 1 @0-12f"}"#)
    })?;
    let mut tracks = Vec::with_capacity(obj.len());
    for (name, body) in obj {
        let here = format!("{path}.{name}");
        let (index, spec) = property(name)
            .ok_or_else(|| Diagnostic::unknown("property", name, property_names()).at(&here))?;
        let keys = match body {
            Value::String(s) => parse_track_string(s, &here, spec, fps)?,
            Value::Array(a) => parse_keys(a, &here, spec, fps)?,
            _ => {
                return Err(
                    Diagnostic::new(Kind::WrongType, "must be a string or a list of keys")
                        .at(&here)
                        .expected(TRACK_EXPECTED),
                );
            }
        };
        tracks.push(AnimTrack {
            property: index,
            keys,
        });
    }
    Ok(tracks)
}

/// Checks `value` against the property's inclusive range.
pub(crate) fn check_range(value: f64, spec: &PropertySpec, path: &str) -> Result<f64> {
    if !value.is_finite() {
        return Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("`{value}` is not a finite number"),
        )
        .at(path)
        .expected("a finite number"));
    }
    match spec.range {
        Some((lo, hi)) if value < lo || value > hi => Err(Diagnostic::new(
            Kind::OutOfRange,
            format!(
                "value {} is outside {}..={}",
                fmt_num(value),
                fmt_num(lo),
                fmt_num(hi)
            ),
        )
        .at(path)
        .expected(format!("a number from {} to {}", fmt_num(lo), fmt_num(hi)))),
        _ => Ok(value),
    }
}

/// Parses a value token of the string form.
fn parse_number(token: &str, path: &str, spec: &PropertySpec) -> Result<f64> {
    let v: f64 = token.parse().map_err(|_| {
        Diagnostic::syntax(format!("`{token}` is not a number"))
            .at(path)
            .expected(STRING_EXPECTED)
    })?;
    check_range(v, spec, path)
}

/// Parses the string form `"<from> -> <to> @<start>-<end> [<curve>]"` into
/// two keys; the curve sits on the first.
pub(crate) fn parse_track_string(
    text: &str,
    path: &str,
    spec: &PropertySpec,
    fps: f64,
) -> Result<Vec<AnimKey>> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let err = |m: String| Diagnostic::syntax(m).at(path).expected(STRING_EXPECTED);
    let arrow = tokens
        .iter()
        .position(|t| *t == ARROW)
        .ok_or_else(|| err(format!("missing `{ARROW}` in `{text}`")))?;
    let range = tokens
        .iter()
        .position(|t| t.starts_with(RANGE_MARK))
        .ok_or_else(|| err(format!("missing `@<start>-<end>` in `{text}`")))?;
    if arrow != 1 {
        return Err(err(format!("`{ARROW}` needs exactly one value before it")));
    }
    if range != 3 {
        return Err(err(
            "`@<start>-<end>` must follow the end value, as the fourth word".to_string(),
        ));
    }
    if tokens.len() > 5 {
        return Err(err(format!("unexpected `{}` after the curve", tokens[5])));
    }
    let from = parse_number(tokens[0], path, spec)?;
    let to = parse_number(tokens[2], path, spec)?;
    let (start, end) = parse_range(tokens[3], path, fps)?;
    let curve = match tokens.get(4) {
        None => Curve::Linear,
        // Always parse, so an unknown name gets the error that lists the
        // known curves.
        Some(t) => Curve::parse(t).map_err(|e| e.under(path))?,
    };
    Ok(vec![
        AnimKey {
            time: start,
            value: from,
            curve,
        },
        AnimKey {
            time: end,
            value: to,
            curve: Curve::Linear,
        },
    ])
}

/// Parses `@<start>-<end>`; the unit of `<end>` also applies to a bare
/// `<start>`.
fn parse_range(token: &str, path: &str, fps: f64) -> Result<(Time, Time)> {
    let err = |m: String| Diagnostic::syntax(m).at(path).expected(RANGE_EXPECTED);
    let body = token.trim_start_matches(RANGE_MARK);
    let (a, b) = body
        .split_once('-')
        .ok_or_else(|| err(format!("`{token}` has no `-` between start and end")))?;
    let unit = |s: &str| s.chars().last().filter(|c| matches!(c, 'f' | 's'));
    let Some(end_unit) = unit(b) else {
        return Err(err(format!(
            "`{token}` needs a unit (`f` or `s`) after the end"
        )));
    };
    let start_text = if unit(a).is_some() {
        a.to_string()
    } else {
        format!("{a}{end_unit}")
    };
    let start = Time::parse(&start_text).map_err(|e| e.under(path))?;
    let end = Time::parse(b).map_err(|e| e.under(path))?;
    if start.to_frames(fps) > end.to_frames(fps) {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("`{token}` starts after it ends"))
                .at(path)
                .expected(RANGE_EXPECTED),
        );
    }
    Ok((start, end))
}

/// Parses the key-list form.
fn parse_keys(list: &[Value], path: &str, spec: &PropertySpec, fps: f64) -> Result<Vec<AnimKey>> {
    if list.is_empty() {
        return Err(Diagnostic::new(Kind::Missing, "needs at least one key")
            .at(path)
            .expected(KEYS_EXPECTED));
    }
    let mut keys: Vec<AnimKey> = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let here = format!("{path}[{i}]");
        let key = parse_key(item, &here, spec)?;
        if let Some(prev) = keys.last()
            && key.time.to_frames(fps) < prev.time.to_frames(fps)
        {
            return Err(
                Diagnostic::new(Kind::Invalid, "key is earlier than the one before it")
                    .at(&here)
                    .expected("keys in time order, earliest first"),
            );
        }
        keys.push(key);
    }
    Ok(keys)
}

/// Parses one `[time, value]` or `[time, value, curve]` key.
fn parse_key(item: &Value, path: &str, spec: &PropertySpec) -> Result<AnimKey> {
    let key_form = r#"[time, value] or [time, value, curve], e.g. ["12f", 1, "ease-out"]"#;
    let parts = match item.as_array() {
        Some(parts) if (2..=3).contains(&parts.len()) => parts,
        // One message covers both failures; the kind tells a value that is
        // not a list from a list of the wrong length.
        list => {
            let kind = match list {
                Some(_) => Kind::Invalid,
                None => Kind::WrongType,
            };
            return Err(Diagnostic::new(kind, "must be a list of 2 or 3 items")
                .at(path)
                .expected(key_form));
        }
    };
    let time = match &parts[0] {
        Value::String(s) => Time::parse(s).map_err(|e| e.under(path))?,
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.is_finite() && f >= 0.0 => Time::Frames(f),
            _ => {
                return Err(
                    Diagnostic::new(Kind::OutOfRange, format!("invalid time {n}"))
                        .at(path)
                        .expected(format!("a frame count of 0 or more, or {TIME_EXPECTED}")),
                );
            }
        },
        other => {
            return Err(
                Diagnostic::new(Kind::WrongType, format!("invalid time {other}"))
                    .at(path)
                    .expected(format!("a frame count, or {TIME_EXPECTED}")),
            );
        }
    };
    let value = parts[1].as_f64().ok_or_else(|| {
        Diagnostic::new(Kind::WrongType, format!("invalid value {}", parts[1]))
            .at(path)
            .expected("a number")
    })?;
    let value = check_range(value, spec, path)?;
    let curve = match parts.get(2) {
        None => Curve::Linear,
        Some(Value::String(s)) => Curve::parse(s).map_err(|e| e.under(path))?,
        Some(other) => {
            return Err(
                Diagnostic::new(Kind::WrongType, format!("invalid curve {other}"))
                    .at(path)
                    .expected("a curve name or cubic(x1,y1,x2,y2)"),
            );
        }
    };
    Ok(AnimKey { time, value, curve })
}

#[cfg(test)]
#[path = "../tests/unit/anim.rs"]
mod tests;
