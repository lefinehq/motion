//! The behaviour shorthand, a layer's `in`: an entrance preset with optional
//! tuning, e.g. `"fade-up 60px 12f ease-out @15f"`.

use std::str::FromStr;

use crate::grammar::{Failure, Parsed, blank0, blank1, ends_with, number, parse_all, word};
use crate::{Diagnostic, Kind, Result};
use winnow::combinator::{alt, cut_err, delimited, eof, peek, preceded, repeat, terminated};
use winnow::error::ErrMode;
use winnow::prelude::*;
use winnow::token::{one_of, rest, take_till};

use crate::Curve;
use crate::curve::CUBIC_PREFIX;
use crate::doc::Form;
use crate::units::{Time, time};
use crate::validate::{ID_RULE, is_valid_id};

pub(crate) const EXPECTED: &str = "`<preset> [<n>px] [<n>f|<n>s] [<curve>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`; a curve is a name such as `ease-out`, `hold`, or `cubic(x1,y1,x2,y2)`";

/// Must be in `NAMED_CURVES`.
const DEFAULT_CURVE: &str = "ease-out";

/// A parsed behaviour: a preset plus its tuning, with defaults filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct Behaviour {
    /// The preset to perform; whether it exists depends on the scene.
    pub preset: String,
    /// Travel distance in pixels; 40 by default.
    pub distance: f64,
    /// How long the motion lasts; 12 frames by default.
    pub duration: Time,
    /// Easing curve; `ease-out` by default.
    pub curve: Curve,
    /// When the motion starts, from the start of the scene; frame 0 by default.
    pub start: Time,
}

impl Behaviour {
    /// The frames the motion starts and ends on at `fps` frames per second.
    pub fn frames(&self, fps: f64) -> (f64, f64) {
        let start = self.start.to_frames(fps);
        (start, start + self.duration.to_frames(fps))
    }

    /// The behaviour of `preset`, each token's field set over the defaults.
    fn tuned(preset: String, tokens: Vec<Token<'_>>) -> Behaviour {
        let defaults = Behaviour {
            preset,
            distance: 40.0,
            duration: Time::Frames(12.0),
            curve: Curve::named(DEFAULT_CURVE).expect("the default curve is in NAMED_CURVES"),
            start: Time::Frames(0.0),
        };
        tokens
            .into_iter()
            .fold(defaults, |behaviour, (tuning, _)| tuning.apply(behaviour))
    }
}

impl FromStr for Behaviour {
    type Err = Diagnostic;

    /// Parses `<preset>` and any of `<n>px`, `<n>f` or `<n>s`, a curve and
    /// `@<time>`, each at most once.
    ///
    /// Tokens are told apart by their shape, never their position, so any
    /// order means the same and no number has to be guessed at.
    fn from_str(text: &str) -> Result<Behaviour> {
        parse_all(text, behaviour, |problem| {
            problem.unwrap_or(Problem::NoPreset).diagnostic()
        })
    }
}

impl Form for Behaviour {
    const FORM: &str = EXPECTED;
}

/// What is wrong with a behaviour string, with the text it is wrong about.
#[derive(Debug, Clone)]
enum Problem<'s> {
    NoPreset,
    /// The preset name is not a valid id.
    Preset(&'s str),
    /// The token has the shape of a field's value but is not a valid one.
    Malformed(Field, &'s str),
    /// The token has no known shape.
    Unknown(&'s str),
    /// Two tokens set the same field: the first and the second.
    Twice(Field, &'s str, &'s str),
    /// The curve token is not a curve, with its own diagnostic.
    Curve(Diagnostic),
}

impl Problem<'_> {
    fn diagnostic(self) -> Diagnostic {
        match self {
            Problem::Curve(diagnostic) => diagnostic,
            Problem::NoPreset => {
                Diagnostic::syntax("a behaviour needs a preset").expected(EXPECTED)
            }
            Problem::Preset(name) => {
                Diagnostic::syntax(format!("`{name}` is not a valid preset name; {ID_RULE}"))
                    .expected(EXPECTED)
            }
            Problem::Malformed(field, token) => {
                Diagnostic::syntax(format!("`{token}` is not a valid {}", field.noun()))
                    .expected(EXPECTED)
            }
            Problem::Unknown(token) => {
                Diagnostic::syntax(format!("unknown token `{token}`")).expected(EXPECTED)
            }
            Problem::Twice(field, first, second) => Diagnostic::new(
                Kind::Conflict,
                format!("{} given twice: `{first}` and `{second}`", field.name()),
            )
            .expected(EXPECTED),
        }
    }
}

/// The field of [`Behaviour`] a tuning token sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Distance,
    Duration,
    Curve,
    Start,
}

impl Field {
    /// As in "duration given twice".
    fn name(self) -> &'static str {
        match self {
            Field::Distance => "distance",
            Field::Duration => "duration",
            Field::Curve => "curve",
            Field::Start => "start",
        }
    }

    /// As in "`@12` is not a valid start time".
    fn noun(self) -> &'static str {
        match self {
            Field::Start => "start time",
            field => field.name(),
        }
    }
}

/// A tuning token and its text.
type Token<'s> = (Tuning, &'s str);

/// One tuning token after the preset.
#[derive(Debug, Clone, Copy)]
enum Tuning {
    Distance(f64),
    Duration(Time),
    Curve(Curve),
    Start(Time),
}

impl Tuning {
    fn field(self) -> Field {
        match self {
            Tuning::Distance(_) => Field::Distance,
            Tuning::Duration(_) => Field::Duration,
            Tuning::Curve(_) => Field::Curve,
            Tuning::Start(_) => Field::Start,
        }
    }

    fn apply(self, behaviour: Behaviour) -> Behaviour {
        match self {
            Tuning::Distance(distance) => Behaviour {
                distance,
                ..behaviour
            },
            Tuning::Duration(duration) => Behaviour {
                duration,
                ..behaviour
            },
            Tuning::Curve(curve) => Behaviour { curve, ..behaviour },
            Tuning::Start(start) => Behaviour { start, ..behaviour },
        }
    }
}

fn behaviour<'s>(input: &mut &'s str) -> Parsed<Behaviour, Problem<'s>> {
    delimited(
        blank0,
        (
            preset,
            repeat(0.., preceded(blank1, token)).try_fold(Vec::new, once_per_field),
        ),
        blank0,
    )
    .map(|(preset, tokens)| Behaviour::tuned(preset.to_owned(), tokens))
    .parse_next(input)
}

/// The preset name; only its shape is checked, since presets depend on the scene.
fn preset<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem<'s>> {
    word.try_map(|name| {
        is_valid_id(name)
            .then_some(name)
            .ok_or(Problem::Preset(name))
    })
    .parse_next(input)
}

/// One word, which must then be a valid tuning token.
fn token<'s>(input: &mut &'s str) -> Parsed<Token<'s>, Problem<'s>> {
    word.and_then(cut_err(tuning))
        .with_taken()
        .parse_next(input)
}

/// Adds `next` to the tokens `read` so far, unless one set the same field before.
fn once_per_field<'s>(
    mut read: Vec<Token<'s>>,
    (tuning, text): Token<'s>,
) -> std::result::Result<Vec<Token<'s>>, Problem<'s>> {
    let field = tuning.field();
    match read.iter().find(|(earlier, _)| earlier.field() == field) {
        Some((_, first)) => Err(Problem::Twice(field, first, text)),
        None => {
            read.push((tuning, text));
            Ok(read)
        }
    }
}

/// One tuning token, told apart by its shape: `@` starts a start time, `px`
/// ends a distance, no digits (or `cubic(`) make a curve and `f` or `s` ends
/// a duration. Each shape commits, so a malformed token is reported as the
/// kind it looks like, and a misspelt curve gets the curve error.
fn tuning<'s>(input: &mut &'s str) -> Parsed<Tuning, Problem<'s>> {
    let token = *input;
    let malformed = |field| Problem::Malformed(field, token);
    alt((
        preceded('@', whole(time).context(malformed(Field::Start))).map(Tuning::Start),
        preceded(
            peek(ends_with("px")),
            whole(terminated(number, "px")).context(malformed(Field::Distance)),
        )
        .map(Tuning::Distance),
        preceded(
            peek(alt((CUBIC_PREFIX.void(), digit_free))),
            cut_err(rest.try_map(|curve: &str| curve.parse().map_err(Problem::Curve))),
        )
        .map(Tuning::Curve),
        preceded(
            peek(ends_with(one_of(['f', 's']))),
            whole(time).context(malformed(Field::Duration)),
        )
        .map(Tuning::Duration),
    ))
    .context(Problem::Unknown(token))
    .parse_next(input)
}

/// `value`, which must end the token; committed, since the shape said what it is.
fn whole<'s, T>(
    value: impl Parser<&'s str, T, ErrMode<Failure<Problem<'s>>>>,
) -> impl Parser<&'s str, T, ErrMode<Failure<Problem<'s>>>> {
    cut_err(terminated(value, eof))
}

fn digit_free<R>(input: &mut &str) -> Parsed<(), R> {
    (take_till(0.., '0'..='9'), eof).void().parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/behaviour.rs"]
mod tests;
