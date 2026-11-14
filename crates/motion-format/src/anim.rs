//! Free-form animation tracks, a layer's `anim`: the typed [`AnimTrack`] and
//! the grammar of the string form `"0 -> 200 @0-30f ease-out"`. Values are
//! typed by the property they animate ([`crate::value`]).

use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, parse_all, word};
use crate::{Diagnostic, Result};
use winnow::combinator::{opt, peek, preceded};
use winnow::prelude::*;

use crate::Curve;
use crate::property::Target;
use crate::units::{SpanProblem, Time, TimeSpan, time_span};
use crate::value::Value;

/// The accepted forms of a track.
pub(crate) const TRACK_EXPECTED: &str = r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out", or a list of keys, each [time, value, curve] or {"t":time,"v":value,"curve":curve,"path":[[ox,oy],[ix,iy]]}"#;
/// The string form only, for errors about its pieces.
const STRING_EXPECTED: &str =
    r#""<from> -> <to> @<start>-<end> [<curve>]", e.g. "0 -> 1 @0-12f ease-out""#;
/// The accepted forms of a key list.
pub(crate) const KEYS_EXPECTED: &str = r#"a list of keys like [[0, 0], ["12f", 1, "ease-out"]]"#;
/// The accepted forms of one key of a key list.
pub(crate) const KEY_EXPECTED: &str = r#"[time, value] or [time, value, curve], e.g. ["12f", 1, "ease-out"], or {"t":time,"v":value,"curve":curve,"path":[[ox,oy],[ix,iy]]}"#;
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

/// One keyframe of a track.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimKey {
    /// When the key sits, in the layer's own time, in the unit the author wrote.
    pub time: Time,
    /// The value at that time, of the track's target type.
    pub value: Value,
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
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) span: Span,
    pub(crate) curve: Curve,
}

impl TrackLine {
    /// The two keys the line stands for, with its values read as `from` and
    /// `to`; the curve sits on the first.
    pub(crate) fn keys(&self, from: Value, to: Value) -> Vec<AnimKey> {
        vec![
            AnimKey {
                time: self.span.start,
                value: from,
                easing: Easing::All(self.curve),
                path: None,
            },
            AnimKey {
                time: self.span.end,
                value: to,
                easing: Easing::All(Curve::Linear),
                path: None,
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
            Problem::Trailing(word) => format!("unexpected `{word}` after the curve"),
            Problem::NotATrack => format!("`{text}` is not a valid track"),
        };
        Diagnostic::syntax(message).expected(STRING_EXPECTED)
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

/// A value word, kept as written: its type is the target's, which
/// `crate::validate` knows.
fn value(input: &mut &str) -> Parsed<String, Problem> {
    word.map(str::to_owned)
        .context(Problem::NoValue)
        .parse_next(input)
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
