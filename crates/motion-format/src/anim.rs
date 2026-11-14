//! Free-form animation tracks, a layer's `anim`: the typed [`AnimTrack`] and
//! the grammar of the string form `"0 -> 200 @0-30f ease-out"`.

use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, parse_all, word};
use crate::{Diagnostic, Result};
use winnow::combinator::{opt, peek, preceded};
use winnow::prelude::*;

use crate::Curve;
use crate::property::Prop;
use crate::units::{Time, amount, unit};

/// The accepted forms of a track.
pub(crate) const TRACK_EXPECTED: &str = r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out", or a list of [time, value] keys"#;
/// The string form only, for errors about its pieces.
const STRING_EXPECTED: &str =
    r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out""#;
/// The accepted forms of a key list.
pub(crate) const KEYS_EXPECTED: &str = r#"a list of keys like [[0, 0], ["12f", 1, "ease-out"]]"#;
/// The accepted forms of one key of a key list.
pub(crate) const KEY_EXPECTED: &str =
    r#"[time, value] or [time, value, curve], e.g. ["12f", 1, "ease-out"]"#;
/// The accepted forms of a range such as `@0-30f`.
pub(crate) const RANGE_EXPECTED: &str = "`@<start>-<end>` with a unit, e.g. `@0-30f`, `@0.5s-1s`";
const ARROW: &str = "->";
const RANGE_MARK: char = '@';

/// One keyframe of a track.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimKey {
    /// When the key sits, in the unit the author wrote.
    pub time: Time,
    /// The property's value at that time.
    pub value: f64,
    /// How the value eases from this key to the next.
    pub curve: Curve,
}

/// The keys of one property.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimTrack {
    /// The property the track animates.
    pub property: Prop,
    /// Keys in non-decreasing time order; never empty.
    pub keys: Vec<AnimKey>,
}

/// The string form of a track as written; `crate::validate` checks its
/// values against the property and its range against the frame rate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrackLine {
    pub(crate) from: f64,
    pub(crate) to: f64,
    pub(crate) span: Span,
    pub(crate) curve: Curve,
}

impl TrackLine {
    /// The two keys the line stands for; the curve sits on the first.
    pub(crate) fn keys(&self) -> Vec<AnimKey> {
        vec![
            AnimKey {
                time: self.span.start,
                value: self.from,
                curve: self.curve,
            },
            AnimKey {
                time: self.span.end,
                value: self.to,
                curve: Curve::Linear,
            },
        ]
    }
}

impl FromStr for TrackLine {
    type Err = Diagnostic;

    fn from_str(text: &str) -> Result<TrackLine> {
        parse_all(text, line, |problem| {
            problem.unwrap_or(Problem::NotATrack).diagnostic(text)
        })
    }
}

/// `@<start>-<end>`. A bare start takes the unit of the end: `@0-30f` is `@0f-30f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    pub(crate) start: Time,
    pub(crate) end: Time,
}

impl FromStr for Span {
    type Err = Diagnostic;

    fn from_str(token: &str) -> Result<Span> {
        parse_all(token, span, |problem| {
            problem.unwrap_or(SpanProblem::NotARange).diagnostic(token)
        })
    }
}

/// Why the string form failed.
#[derive(Debug, Clone)]
enum Problem {
    NotNumber(String),
    NoArrow,
    NoRange,
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
            Problem::NotNumber(word) => format!("`{word}` is not a number"),
            Problem::NoArrow => format!("missing `{ARROW}` after the start value in `{text}`"),
            Problem::NoRange => format!("missing `@<start>-<end>` after the end value in `{text}`"),
            Problem::Trailing(word) => format!("unexpected `{word}` after the curve"),
            Problem::NotATrack => format!("`{text}` is not a valid track"),
        };
        Diagnostic::syntax(message).expected(STRING_EXPECTED)
    }
}

/// Why a range failed.
#[derive(Debug, Clone, Copy)]
enum SpanProblem {
    NoDash,
    NoUnit,
    NotARange,
}

impl SpanProblem {
    fn diagnostic(self, token: &str) -> Diagnostic {
        let message = match self {
            SpanProblem::NoDash => format!("`{token}` has no `-` between start and end"),
            SpanProblem::NoUnit => format!("`{token}` needs a unit (`f` or `s`) after the end"),
            SpanProblem::NotARange => format!("`{token}` is not a valid range"),
        };
        Diagnostic::syntax(message).expected(RANGE_EXPECTED)
    }
}

/// The whole string form: whitespace-separated words in a fixed order.
fn line(input: &mut &str) -> Parsed<TrackLine, Problem> {
    let (from, (), to, span, curve, ()) = (
        preceded(blank0, value),
        preceded(blank1, ARROW).void().context(Problem::NoArrow),
        preceded(blank1, value),
        range,
        curve,
        nothing_after,
    )
        .parse_next(input)?;
    Ok(TrackLine {
        from,
        to,
        span,
        curve,
    })
}

/// A value word, read as `f64` reads it, so a non-finite value gets the range error later.
fn value(input: &mut &str) -> Parsed<f64, Problem> {
    word.try_map(|word: &str| {
        word.parse::<f64>()
            .map_err(|_| Problem::NotNumber(word.to_owned()))
    })
    .parse_next(input)
}

/// The word that starts with `@`, read as a [`Span`].
fn range(input: &mut &str) -> Parsed<Span, Problem> {
    preceded(blank1, preceded(peek(RANGE_MARK), word))
        .context(Problem::NoRange)
        .try_map(|word: &str| word.parse::<Span>().map_err(Problem::Part))
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
    let (start, start_unit) = preceded(RANGE_MARK, (amount, opt(unit))).parse_next(input)?;
    '-'.context(SpanProblem::NoDash).parse_next(input)?;
    let (end, end_unit) = (amount, unit.context(SpanProblem::NoUnit)).parse_next(input)?;
    Ok(Span {
        start: start_unit.unwrap_or(end_unit)(start),
        end: end_unit(end),
    })
}

#[cfg(test)]
#[path = "../tests/unit/anim.rs"]
mod tests;
