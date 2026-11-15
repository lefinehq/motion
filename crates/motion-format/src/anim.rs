//! Free-form animation tracks, a layer's `anim`: the typed [`AnimTrack`] and
//! the grammar of the string form `"0 -> 200 @0-30f ease-out"`, which may
//! chain values (`"0 -> 200@40% -> 150 @0-30f"`). Values are typed by the
//! property they animate ([`crate::value`]), and may be written against its
//! resting value (`rest+40`, `rest*1.2`).

use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, parse_all, word};
use crate::{Diagnostic, Result};
use winnow::combinator::{cut_err, opt, peek, preceded, repeat};
use winnow::prelude::*;

use crate::property::Target;
use crate::units::{SpanProblem, Time, TimeSpan, time_span};
use crate::value::{Value, ValueType};
use crate::{Curve, fmt_num};

/// The accepted forms of a track.
pub(crate) const TRACK_EXPECTED: &str = r#""<value> -> <value>[@<n>%] ... -> <value> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out" or "rest+40 -> rest @0-12f", or a list of keys, each [time, value, curve] or {"t":time,"v":value,"curve":curve,"path":[[out-x,out-y],[in-x,in-y]]}"#;
/// The string form only, for errors about its pieces.
const STRING_EXPECTED: &str = r#""<value> -> <value>[@<n>%] ... -> <value> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out", "0 -> 200@40% -> 150 @0-30f" or "rest+40 -> rest @0-12f""#;
/// The accepted forms of a value written against the rest.
pub(crate) const REST_EXPECTED: &str = "`rest`, `rest+<value>`, `rest-<value>` or `rest*<factor>`, e.g. `rest+40`, `rest+0,-40` or `rest*1.2`, on a number or a point";
/// The word a value written against the rest starts with.
pub(crate) const REST: &str = "rest";
/// The accepted forms of a key list.
pub(crate) const KEYS_EXPECTED: &str = r#"a list of keys like [[0, 0], ["12f", 1, "ease-out"]]"#;
/// The accepted forms of one key of a key list.
pub(crate) const KEY_EXPECTED: &str = r#"[time, value] or [time, value, curve], e.g. ["12f", 1, "ease-out"], or {"t":time,"v":value,"curve":curve,"path":[[out-x,out-y],[in-x,in-y]]}"#;
/// The accepted forms of a key's curve.
pub(crate) const EASING_EXPECTED: &str = r#"a curve such as "ease-out", or a list of curves, one per component, e.g. ["ease-out","linear"]"#;
/// The accepted form of a key's path tangents.
pub(crate) const PATH_EXPECTED: &str = "[[out-x, out-y], [in-x, in-y]]: the bend leaving this key, as an offset from its value, and the bend arriving at the next key, as an offset from that key's value";
pub(crate) use crate::units::RANGE_EXPECTED;
const ARROW: &str = "->";
const RANGE_MARK: char = '@';

/// How a span between two keys eases.
#[derive(Debug, Clone, PartialEq)]
pub enum Easing {
    /// One curve for the whole value; on a spatial property it times the
    /// move along the path.
    All(Curve),
    /// One curve per component, for a value that is not spatial.
    Each(Vec<Curve>),
}

impl Easing {
    /// The curve of component `i`.
    pub fn of(&self, i: usize) -> Curve {
        match self {
            Easing::All(curve) => *curve,
            Easing::Each(curves) => curves[i],
        }
    }
}

/// A key's value: as written, or against the property's resting value on
/// its layer or item, which is known once the scene compiles.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyValue {
    /// A value of the target's type.
    Set(Value),
    /// The resting value, changed.
    Rest(Change),
}

/// How a value written against the rest changes it.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// `rest`: the resting value itself.
    Same,
    /// `rest+<value>` or `rest-<value>`: moved by a value of the target's
    /// type, negated for `-`.
    Plus(Value),
    /// `rest*<factor>`: every component scaled.
    Times(f64),
}

impl KeyValue {
    /// The value, with `rest` the target's resting value.
    ///
    /// # Panics
    /// If a change by a value has another type than `rest`, which
    /// validation rules out.
    pub fn resolve(&self, rest: &Value) -> Value {
        let numbers = rest.components();
        match self {
            KeyValue::Set(set) => set.clone(),
            KeyValue::Rest(Change::Same) => rest.clone(),
            KeyValue::Rest(Change::Plus(by)) => {
                assert_eq!(
                    by.value_type(),
                    rest.value_type(),
                    "a change of the rest's type"
                );
                let moved: Vec<f64> = numbers
                    .iter()
                    .zip(by.components())
                    .map(|(v, d)| v + d)
                    .collect();
                rest.with_components(&moved)
            }
            KeyValue::Rest(Change::Times(factor)) => {
                let scaled: Vec<f64> = numbers.iter().map(|v| v * factor).collect();
                rest.with_components(&scaled)
            }
        }
    }

    /// The value as written, if it does not depend on the rest.
    pub fn set(&self) -> Option<&Value> {
        match self {
            KeyValue::Set(value) => Some(value),
            KeyValue::Rest(_) => None,
        }
    }
}

/// One keyframe of a track.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimKey {
    /// When the key sits, in the layer's own time, in the unit the author wrote.
    pub time: Time,
    /// The value at that time, of the track's target type, or against its rest.
    pub value: KeyValue,
    /// How the value eases from this key to the next.
    pub easing: Easing,
    /// On a spatial property, how the path bends between this key and the
    /// next: the offset of the leaving handle from this key's value, and of the
    /// arriving handle from the next key's value. Without it the path is straight.
    pub path: Option<[[f64; 2]; 2]>,
}

/// The keys of one property, or of one component of it.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimTrack {
    /// What the track animates.
    pub target: Target,
    /// Keys in non-decreasing time order; never empty.
    pub keys: Vec<AnimKey>,
}

/// The string form of a track as written; `crate::validate` reads its values
/// as the target's type and checks its range against the frame rate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrackLine {
    /// Each value word, at least two, with where it falls in the span as a
    /// share from 0 to 1 when it says so; only a middle value may.
    pub(crate) values: Vec<(String, Option<f64>)>,
    pub(crate) span: Span,
    pub(crate) curve: Curve,
}

impl TrackLine {
    /// The keys the line stands for, with its values read as `values`: the
    /// first at the span's start, the last at its end, and those between at
    /// their positions or evenly between their neighbours. Every key but
    /// the last takes the curve.
    pub(crate) fn keys(&self, values: Vec<KeyValue>, fps: f64) -> Vec<AnimKey> {
        let shares = self.shares();
        let last = values.len() - 1;
        values
            .into_iter()
            .zip(shares)
            .enumerate()
            .map(|(i, (value, share))| AnimKey {
                time: self.span.at(share, fps),
                value,
                easing: Easing::All(match i == last {
                    true => Curve::Linear,
                    false => self.curve,
                }),
                path: None,
            })
            .collect()
    }

    /// Where each value falls in the span, 0 to 1: the given positions, and
    /// the others evenly between the given ones around them.
    fn shares(&self) -> Vec<f64> {
        let n = self.values.len();
        let mut shares: Vec<Option<f64>> = self.values.iter().map(|(_, at)| *at).collect();
        shares[0] = Some(0.0);
        shares[n - 1] = Some(1.0);
        let mut from = 0;
        for to in 1..n {
            let Some(end) = shares[to] else { continue };
            let start = shares[from].expect("a given share");
            let steps = (to - from) as f64;
            for (k, share) in shares.iter_mut().enumerate().take(to).skip(from + 1) {
                *share = Some(start + (end - start) * (k - from) as f64 / steps);
            }
            from = to;
        }
        shares
            .into_iter()
            .map(|s| s.expect("every share filled"))
            .collect()
    }
}

impl TimeSpan {
    /// The time at `share` of the span, from 0 at its start to 1 at its
    /// end, in its unit when both ends share one, else in frames.
    fn at(self, share: f64, fps: f64) -> Time {
        let lerp = |a: f64, b: f64| a + (b - a) * share;
        match (self.start, self.end) {
            _ if share == 0.0 => self.start,
            _ if share == 1.0 => self.end,
            (Time::Frames(a), Time::Frames(b)) => Time::Frames(lerp(a, b)),
            (Time::Seconds(a), Time::Seconds(b)) => Time::Seconds(lerp(a, b)),
            (a, b) => Time::Frames(lerp(a.to_frames(fps), b.to_frames(fps))),
        }
    }
}

/// Reads a value word written against the rest, for a target of type `ty`;
/// `None` when the word does not start with `rest`.
pub(crate) fn rest_word(word: &str, ty: ValueType, broadcast: bool) -> Option<Result<Change>> {
    let after = word.strip_prefix(REST)?;
    let wrong = || {
        Diagnostic::syntax(format!("`{word}` is not a value against the rest"))
            .expected(REST_EXPECTED)
    };
    if !matches!(ty, ValueType::Number | ValueType::Point) {
        return Some(Err(Diagnostic::new(
            crate::Kind::Invalid,
            format!("`{word}`: only a number or a point can be written against its rest"),
        )
        .expected(REST_EXPECTED)));
    }
    let change = match after.chars().next() {
        None => Ok(Change::Same),
        Some(sign @ ('+' | '-')) => Value::parse(&after[1..], ty, broadcast)
            .map_err(|_| wrong())
            .map(|by| match sign {
                '-' => {
                    let negated: Vec<f64> = by.components().iter().map(|v| -v).collect();
                    Change::Plus(by.with_components(&negated))
                }
                _ => Change::Plus(by),
            }),
        Some('*') => after[1..]
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(Change::Times)
            .ok_or_else(wrong),
        Some(_) => Err(wrong()),
    };
    Some(change)
}

impl FromStr for TrackLine {
    type Err = Diagnostic;

    fn from_str(text: &str) -> Result<TrackLine> {
        parse_all(text, line, |problem| {
            problem.unwrap_or(Problem::NotATrack).diagnostic(text)
        })
    }
}

/// `@<start>-<end>`: a [`TimeSpan`] after `@`.
pub(crate) type Span = TimeSpan;

impl TimeSpan {
    /// Reads the range form `@<start>-<end>`.
    pub(crate) fn from_range(token: &str) -> Result<TimeSpan> {
        parse_all(token, span, |problem| {
            problem.unwrap_or(SpanProblem::NotARange).diagnostic(token)
        })
    }
}

/// Why the string form failed.
#[derive(Debug, Clone)]
enum Problem {
    NoValue,
    NoArrow,
    NoRange,
    /// A further `->` with no value after it.
    Dangling,
    /// A value's position is not a percentage.
    Position(String),
    /// The first or last value says where it falls.
    EndPosition(String),
    /// Positions that do not run forward strictly between 0% and 100%.
    Positions(String),
    /// A word after the curve.
    Trailing(String),
    /// The range or the curve failed, with its own diagnostic.
    Part(Diagnostic),
    NotATrack,
}

impl Problem {
    fn diagnostic(self, text: &str) -> Diagnostic {
        let message = match self {
            Problem::Part(diagnostic) => return diagnostic,
            Problem::NoValue => format!("missing the start value in `{text}`"),
            Problem::NoArrow => format!("missing `{ARROW}` after the start value in `{text}`"),
            Problem::NoRange => format!("missing `@<start>-<end>` after the end value in `{text}`"),
            Problem::Dangling => format!("missing a value after the last `{ARROW}` in `{text}`"),
            Problem::Position(word) => {
                format!("`{word}` is not a position; a value says where it falls as `@<n>%`")
            }
            Problem::EndPosition(word) => format!(
                "`{word}`: the first value falls at the start and the last at the end, so only a value between them takes a position"
            ),
            Problem::Positions(word) => format!(
                "`{word}`: positions run forward, strictly between 0% and 100%, each after the one before it"
            ),
            Problem::Trailing(word) => format!("unexpected `{word}` after the curve"),
            Problem::NotATrack => format!("`{text}` is not a valid track"),
        };
        Diagnostic::syntax(message).expected(STRING_EXPECTED)
    }
}

/// The whole string form: whitespace-separated words in a fixed order.
fn line(input: &mut &str) -> Parsed<TrackLine, Problem> {
    (
        preceded(blank0, value),
        preceded(blank1, ARROW).void().context(Problem::NoArrow),
        preceded(blank1, value),
        repeat(0.., preceded((blank1, ARROW, blank1), cut_err(next_value))).fold(
            Vec::new,
            |mut all: Vec<_>, v| {
                all.push(v);
                all
            },
        ),
        range,
        curve,
        nothing_after,
    )
        .try_map(|(first, (), second, more, span, curve, ())| {
            let values: Vec<_> = [first, second].into_iter().chain(more).collect();
            positions_run_forward(&values).map(|()| TrackLine {
                values,
                span,
                curve,
            })
        })
        .parse_next(input)
}

/// Only middle values take positions, and those run forward strictly
/// inside the span.
fn positions_run_forward(values: &[(String, Option<f64>)]) -> std::result::Result<(), Problem> {
    let last = values.len() - 1;
    let mut after = 0.0;
    for (i, (value, at)) in values.iter().enumerate() {
        let Some(at) = *at else { continue };
        let word = format!("{value}{RANGE_MARK}{}%", fmt_num(at * 100.0));
        if i == 0 || i == last {
            return Err(Problem::EndPosition(word));
        }
        if at <= after || at >= 1.0 {
            return Err(Problem::Positions(word));
        }
        after = at;
    }
    Ok(())
}

/// A value word, kept as written since its type is the target's, which
/// `crate::validate` knows, with its position when it ends in `@<n>%`.
fn value(input: &mut &str) -> Parsed<(String, Option<f64>), Problem> {
    word.context(Problem::NoValue)
        .try_map(positioned)
        .parse_next(input)
}

/// A value after a further `->`, which must not be the range.
fn next_value(input: &mut &str) -> Parsed<(String, Option<f64>), Problem> {
    word.verify(|word: &str| !word.starts_with(RANGE_MARK))
        .context(Problem::Dangling)
        .try_map(positioned)
        .parse_next(input)
}

/// A value word and its position, if it ends in `@<n>%`.
fn positioned(word: &str) -> std::result::Result<(String, Option<f64>), Problem> {
    match word.split_once(RANGE_MARK) {
        Some((value, at)) if !value.is_empty() => at
            .strip_suffix('%')
            .and_then(|n| n.parse::<f64>().ok())
            .filter(|n| n.is_finite())
            .map(|n| (value.to_owned(), Some(n / 100.0)))
            .ok_or_else(|| Problem::Position(word.to_owned())),
        _ => Ok((word.to_owned(), None)),
    }
}

/// The word that starts with `@`, read as a [`Span`].
fn range(input: &mut &str) -> Parsed<Span, Problem> {
    preceded(blank1, preceded(peek(RANGE_MARK), word))
        .context(Problem::NoRange)
        .try_map(|word: &str| TimeSpan::from_range(word).map_err(Problem::Part))
        .parse_next(input)
}

/// The optional curve word, read whole before it is parsed so that a bad
/// curve reports the curve's error rather than "unexpected word".
fn curve(input: &mut &str) -> Parsed<Curve, Problem> {
    opt(preceded(blank1, word))
        .try_map(|word: Option<&str>| {
            word.map_or(Ok(Curve::Linear), |w| w.parse::<Curve>())
                .map_err(Problem::Part)
        })
        .parse_next(input)
}

/// The end of the line, after optional whitespace; a further word is named.
fn nothing_after(input: &mut &str) -> Parsed<(), Problem> {
    (
        opt(preceded(blank1, word)).try_map(|word: Option<&str>| {
            word.map_or(Ok(()), |w| Err(Problem::Trailing(w.to_owned())))
        }),
        blank0,
    )
        .void()
        .parse_next(input)
}

fn span(input: &mut &str) -> Parsed<Span, SpanProblem> {
    preceded(RANGE_MARK, time_span).parse_next(input)
}

#[cfg(test)]
#[path = "../tests/unit/anim.rs"]
mod tests;
