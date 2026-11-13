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

use super::lex::{self, Failure, Parsed, blank0, blank1, ends_with, number, time, word};
use crate::Curve;
use crate::curve::CUBIC_PREFIX;
use crate::parse::{ID_RULE, is_valid_id};
use crate::units::Time;

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
        // Every other failure is diagnosed where it happens, so a plain
        // syntax failure can only mean there was no word at all.
        lex::parse_all(text, behaviour, |_| {
            Diagnostic::syntax("a behaviour needs a preset").expected(EXPECTED)
        })
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

impl FromStr for Tuning {
    type Err = Diagnostic;

    /// Parses one whitespace-free token.
    fn from_str(token: &str) -> Result<Tuning> {
        lex::parse_all(token, tuning, |field| {
            let message = match field {
                Some(field) => format!("`{token}` is not a valid {}", field.noun()),
                None => format!("unknown token `{token}`"),
            };
            Diagnostic::syntax(message).expected(EXPECTED)
        })
    }
}

/// The preset, then tuning tokens in any order, each kind at most once.
fn behaviour(input: &mut &str) -> Parsed<Behaviour> {
    delimited(
        blank0,
        (
            preset,
            repeat(0.., preceded(blank1, token)).try_fold(Vec::new, once_per_field),
        ),
        blank0,
    )
    .map(|(preset, tokens)| Behaviour::tuned(preset, tokens))
    .parse_next(input)
}

/// The preset name. Only its shape is checked here: which presets exist
/// depends on the scene, and the scene parser checks that.
fn preset(input: &mut &str) -> Parsed<String> {
    word.try_map(|name: &str| {
        is_valid_id(name).then(|| name.to_owned()).ok_or_else(|| {
            Diagnostic::syntax(format!("`{name}` is not a valid preset name; {ID_RULE}"))
                .expected(EXPECTED)
        })
    })
    .parse_next(input)
}

/// One word read as a tuning token, with its text. Once there is a word it
/// must be a valid token, so its diagnostic stands.
fn token<'s>(input: &mut &'s str) -> Parsed<Token<'s>> {
    word.and_then(cut_err(rest.try_map(str::parse::<Tuning>)))
        .with_taken()
        .parse_next(input)
}

/// Adds `next` to the tokens `read` so far, unless a token for the same
/// field came before it: the one "given twice" check, for every field.
fn once_per_field<'s>(mut read: Vec<Token<'s>>, next: Token<'s>) -> Result<Vec<Token<'s>>> {
    let field = next.0.field();
    if let Some((_, first)) = read.iter().find(|(tuning, _)| tuning.field() == field) {
        return Err(Diagnostic::new(
            Kind::Conflict,
            format!("{} given twice: `{first}` and `{}`", field.name(), next.1),
        )
        .expected(EXPECTED));
    }
    read.push(next);
    Ok(read)
}

/// One tuning token, told apart by its shape: `@` starts a start time, a
/// `px` ending makes a distance, a token without digits (or starting
/// `cubic(`) is a curve, and an `f` or `s` ending makes a duration. Each
/// shape commits, so a malformed token is reported as the kind it looks
/// like; a misspelt curve gets the curve error that lists the names.
fn tuning(input: &mut &str) -> Parsed<Tuning, Field> {
    alt((
        preceded('@', whole(time).context(Field::Start)).map(Tuning::Start),
        preceded(
            peek(ends_with("px")),
            whole(terminated(number, "px")).context(Field::Distance),
        )
        .map(Tuning::Distance),
        preceded(
            peek(alt((CUBIC_PREFIX.void(), digit_free))),
            cut_err(rest.try_map(str::parse::<Curve>)),
        )
        .map(Tuning::Curve),
        preceded(
            peek(ends_with(one_of(['f', 's']))),
            whole(time).context(Field::Duration),
        )
        .map(Tuning::Duration),
    ))
    .parse_next(input)
}

/// `value`, which must then end the token; committed, since the token's
/// shape has already said what it is.
fn whole<'s, T>(
    value: impl Parser<&'s str, T, ErrMode<Failure<Field>>>,
) -> impl Parser<&'s str, T, ErrMode<Failure<Field>>> {
    cut_err(terminated(value, eof))
}

/// A token with no digit in it.
fn digit_free(input: &mut &str) -> Parsed<(), Field> {
    (take_till(0.., '0'..='9'), eof).void().parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/behaviour.rs"]
mod tests;
