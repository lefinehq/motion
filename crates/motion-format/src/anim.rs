//! Free-form animation tracks: the `anim` key of a layer.
//!
//! Part of step 1 (Read). `anim` maps a property name to either a one-line
//! string (`"0 -> 200 @0-30f ease-out"`) or a list of keyframes. Both forms
//! produce the same [`AnimTrack`]. This module holds the typed tracks and the
//! `winnow` grammar of the string form; the JSON shapes are read in
//! `crate::doc`, and the rules that need the property registry or the frame
//! rate (ranges, key order) are checked in `crate::validate`.

use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use motion_grammar::{Parsed, blank0, blank1, parse_all, word};
use winnow::combinator::{opt, peek, preceded};
use winnow::prelude::*;

use crate::Curve;
use crate::units::{Time, amount, unit};

/// The accepted forms of a track, for error messages.
pub(crate) const TRACK_EXPECTED: &str = r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out", or a list of [time, value] keys"#;
/// The string form only, for errors that are about its pieces.
const STRING_EXPECTED: &str =
    r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out""#;
/// The accepted forms of a key list.
pub(crate) const KEYS_EXPECTED: &str = r#"a list of keys like [[0, 0], ["12f", 1, "ease-out"]]"#;
/// The accepted forms of one key of a key list.
pub(crate) const KEY_EXPECTED: &str =
    r#"[time, value] or [time, value, curve], e.g. ["12f", 1, "ease-out"]"#;
/// The accepted forms of a range such as `@0-30f`.
pub(crate) const RANGE_EXPECTED: &str = "`@<start>-<end>` with a unit, e.g. `@0-30f`, `@0.5s-1s`";
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

/// The string form of a track, `"<from> -> <to> @<start>-<end> [<curve>]"`,
/// as written. Whether the values fit the property and the range runs
/// forward is checked in `crate::validate`, which knows both.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrackLine {
    /// The value at the start of the range.
    pub(crate) from: f64,
    /// The value at the end of the range.
    pub(crate) to: f64,
    /// When the change starts and ends.
    pub(crate) span: Span,
    /// How the value eases from `from` to `to`; `linear` when left out.
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

    /// Parses `"<from> -> <to> @<start>-<end> [<curve>]"`.
    fn from_str(text: &str) -> Result<TrackLine> {
        parse_all(text, line, |problem| {
            let message = match problem {
                Some(Problem::Part(diagnostic)) => return diagnostic,
                Some(Problem::NotNumber(word)) => format!("`{word}` is not a number"),
                Some(Problem::NoArrow) => {
                    format!("missing `{ARROW}` after the start value in `{text}`")
                }
                Some(Problem::NoRange) => {
                    format!("missing `@<start>-<end>` after the end value in `{text}`")
                }
                Some(Problem::Trailing(word)) => format!("unexpected `{word}` after the curve"),
                None => format!("`{text}` is not a valid track"),
            };
            Diagnostic::syntax(message).expected(STRING_EXPECTED)
        })
    }
}

/// `@<start>-<end>`: when a track's change starts and ends. A bare start
/// takes the unit of the end, so `@0-30f` is `@0f-30f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    /// When the change starts.
    pub(crate) start: Time,
    /// When the change ends.
    pub(crate) end: Time,
}

impl FromStr for Span {
    type Err = Diagnostic;

    /// Parses `@<start>-<end>`, e.g. `@0-30f` or `@0.5s-1s`.
    fn from_str(token: &str) -> Result<Span> {
        parse_all(token, span, |problem| {
            let message = match problem {
                Some(SpanProblem::NoDash) => {
                    format!("`{token}` has no `-` between start and end")
                }
                Some(SpanProblem::NoUnit) => {
                    format!("`{token}` needs a unit (`f` or `s`) after the end")
                }
                None => format!("`{token}` is not a valid range"),
            };
            Diagnostic::syntax(message).expected(RANGE_EXPECTED)
        })
    }
}

/// Why the string form failed, when a part of the grammar says so.
#[derive(Debug, Clone)]
enum Problem {
    /// A value word that is not a number.
    NotNumber(String),
    /// No `->` after the start value.
    NoArrow,
    /// No `@<start>-<end>` word after the end value.
    NoRange,
    /// A word after the curve.
    Trailing(String),
    /// A nested shorthand (the range or the curve) failed; its diagnostic
    /// already says how.
    Part(Diagnostic),
}

/// Why a range failed.
#[derive(Debug, Clone, Copy)]
enum SpanProblem {
    /// No `-` after the start.
    NoDash,
    /// The end has no `f` or `s`.
    NoUnit,
}

/// The whole string form: words separated by whitespace, in a fixed order.
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

/// A value word, read as `f64` reads it. Its range is checked later, so a
/// non-finite value gets the range error rather than this one.
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

/// The optional curve word; `linear` when left out. The word is read before
/// it is parsed, so a bad curve reports the curve's error instead of
/// backtracking to "unexpected word".
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

/// `@<start>-<end>`, where the start may leave out its unit.
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
