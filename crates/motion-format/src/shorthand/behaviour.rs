//! The behaviour shorthand: an entrance preset with optional tuning tokens,
//! e.g. `"fade-up 60px 12f ease-out @15f"`.
//!
//! Part of step 1 (Read): parses a layer's `in` string into a [`Behaviour`].
//! The preset name is a plain string here; the scene parser resolves it.
//! Step 2 turns it into an entrance on the compiled graph.

use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Kind, Result};
use winnow::combinator::{alt, cut_err, delimited, eof, peek, preceded, repeat, terminated};
use winnow::error::ErrMode;
use winnow::prelude::*;
use winnow::token::{one_of, rest, take_till};

use crate::Curve;
use crate::curve::CUBIC_PREFIX;
use crate::lex::{self, Failure, Parsed, blank0, blank1, ends_with, number, word};
use crate::units::{Time, time};
use crate::validate::{ID_RULE, is_valid_id};

pub(crate) const EXPECTED: &str = "`<preset> [<n>px] [<n>f|<n>s] [<curve>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`; a curve is a name such as `ease-out`, `hold`, or `cubic(x1,y1,x2,y2)`";

/// Name of the default curve, which must be in `NAMED_CURVES`.
const DEFAULT_CURVE: &str = "ease-out";

/// A parsed behaviour: a preset plus its tuning, with defaults filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct Behaviour {
    /// The name of the preset to perform; checked against the scene's presets
    /// when the scene is parsed.
    pub preset: String,
    /// Travel distance in pixels; 40 by default.
    pub distance: f64,
    /// How long the motion lasts; 12 frames by default.
    pub duration: Time,
    /// Easing curve; `ease-out` by default.
    pub curve: Curve,
    /// When the motion starts, measured from the start of the scene; frame 0 by default.
    pub start: Time,
}

impl Behaviour {
    /// The behaviour of `preset` with each token's field set over the
    /// defaults.
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

    /// Parses `<preset>` followed by any of `<n>px` (distance), `<n>f` or
    /// `<n>s` (duration), a curve and `@<time>` (start).
    ///
    /// Tokens after the preset are classified by their shape, never by their
    /// position. An agent can write them in any order and still get the same
    /// result, and a given string has exactly one meaning, so there is no
    /// "which number is the distance?" guess to get wrong. Each kind may
    /// appear once.
    fn from_str(text: &str) -> Result<Behaviour> {
        lex::parse_all(text, behaviour, |problem| {
            // Every other failure names its problem where it happens, so a
            // failure without one can only mean there was no word at all.
            let (kind, message) = match problem.unwrap_or(Problem::NoPreset) {
                Problem::Curve(diagnostic) => return diagnostic,
                Problem::NoPreset => (Kind::Syntax, "a behaviour needs a preset".to_owned()),
                Problem::Preset(name) => (
                    Kind::Syntax,
                    format!("`{name}` is not a valid preset name; {ID_RULE}"),
                ),
                Problem::Malformed(field, token) => (
                    Kind::Syntax,
                    format!("`{token}` is not a valid {}", field.noun()),
                ),
                Problem::Unknown(token) => (Kind::Syntax, format!("unknown token `{token}`")),
                Problem::Twice(field, first, second) => (
                    Kind::Conflict,
                    format!("{} given twice: `{first}` and `{second}`", field.name()),
                ),
            };
            Diagnostic::new(kind, message).expected(EXPECTED)
        })
    }
}

/// What is wrong with a behaviour string, with the text it is wrong about.
#[derive(Debug, Clone)]
enum Problem<'s> {
    /// There is no word to name the preset.
    NoPreset,
    /// The preset name is not a valid id.
    Preset(&'s str),
    /// The token has the shape of a field's value but is not a valid one.
    Malformed(Field, &'s str),
    /// The token has no known shape.
    Unknown(&'s str),
    /// Two tokens set the same field: the first and the second.
    Twice(Field, &'s str, &'s str),
    /// The curve token is not a curve; its own diagnostic says why.
    Curve(Diagnostic),
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
    /// The field's name, as in "duration given twice".
    fn name(self) -> &'static str {
        match self {
            Field::Distance => "distance",
            Field::Duration => "duration",
            Field::Curve => "curve",
            Field::Start => "start",
        }
    }

    /// What a malformed token for this field failed to be, as in "`@12` is
    /// not a valid start time".
    fn noun(self) -> &'static str {
        match self {
            Field::Start => "start time",
            field => field.name(),
        }
    }
}

/// A tuning token as read, with its text for messages.
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
    /// The field this token sets.
    fn field(self) -> Field {
        match self {
            Tuning::Distance(_) => Field::Distance,
            Tuning::Duration(_) => Field::Duration,
            Tuning::Curve(_) => Field::Curve,
            Tuning::Start(_) => Field::Start,
        }
    }

    /// `behaviour` with this token's field set.
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

/// The preset, then tuning tokens in any order, each kind at most once.
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

/// The preset name. Only its shape is checked here: which presets exist
/// depends on the scene, and the scene parser checks that.
fn preset<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem<'s>> {
    word.try_map(|name| {
        is_valid_id(name)
            .then_some(name)
            .ok_or(Problem::Preset(name))
    })
    .parse_next(input)
}

/// One word read as a tuning token, with its text. Once there is a word it
/// must be a valid token, so its problem stands.
fn token<'s>(input: &mut &'s str) -> Parsed<Token<'s>, Problem<'s>> {
    word.and_then(cut_err(tuning))
        .with_taken()
        .parse_next(input)
}

/// Adds `next` to the tokens `read` so far, unless a token for the same
/// field came before it: the one "given twice" check, for every field.
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

/// One tuning token, the whole of `input`, told apart by its shape: `@`
/// starts a start time, a `px` ending makes a distance, a token without
/// digits (or starting `cubic(`) is a curve, and an `f` or `s` ending makes
/// a duration. Each shape commits, so a malformed token is reported as the
/// kind it looks like; a misspelt curve gets the curve error that lists the
/// names.
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

/// `value`, which must then end the token; committed, since the token's
/// shape has already said what it is.
fn whole<'s, T>(
    value: impl Parser<&'s str, T, ErrMode<Failure<Problem<'s>>>>,
) -> impl Parser<&'s str, T, ErrMode<Failure<Problem<'s>>>> {
    cut_err(terminated(value, eof))
}

/// A token with no digit in it.
fn digit_free<R>(input: &mut &str) -> Parsed<(), R> {
    (take_till(0.., '0'..='9'), eof).void().parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/behaviour.rs"]
mod tests;
