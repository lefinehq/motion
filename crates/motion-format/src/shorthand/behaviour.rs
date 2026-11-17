//! The behaviour shorthand, a layer's `in`: an entrance preset with optional
//! tuning, e.g. `"fade-up 60px 12f ease-out @15f"`, and on a text layer the
//! parts that enter one after another, e.g. `"fade-up word stagger 3f"`.

use std::str::FromStr;

use crate::grammar::{Failure, Parsed, blank0, blank1, ends_with, number, parse_all, word};
use crate::{Diagnostic, Kind, Result};
use winnow::combinator::{alt, cut_err, delimited, eof, peek, preceded, repeat, terminated};
use winnow::error::ErrMode;
use winnow::prelude::*;
use winnow::token::{one_of, rest, take_till};

use crate::Curve;
use crate::address::{ID_RULE, Id};
use crate::curve::CUBIC_PREFIX;
use crate::doc::Form;
use crate::item::ANIMATORS;
use crate::units::{Time, time};

pub(crate) const EXPECTED: &str = "`<preset> [<n>px] [<n>f|<n>s] [<curve>] [@<time>] [char|char-no-space|word|line] [stagger <time>] [first|last|center|edges|random]`, e.g. `fade-up 40px 12f ease-out @0f` or `fade-up word stagger 3f`; a curve is a name such as `ease-out`, `out-expo` or `hold`, `out-back(2)`, `steps(4)`, `spring(.3)`, or `cubic(x1,y1,x2,y2)`";

/// Must be in `NAMED_CURVES`.
const DEFAULT_CURVE: &str = "ease-out";

/// The word that starts a stagger clause.
const STAGGER: &str = "stagger";

/// The parts a behaviour can move: a text animator's `by` choices.
static PARTS: &[&str] = ANIMATORS[0].settings[0].choices;

/// The orders of a stagger, `first` the default: a text animator's `order` choices.
static ORDERS: &[&str] = ANIMATORS[0].settings[1].choices;

/// A parsed behaviour: a preset plus its tuning, with defaults filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct Behaviour {
    /// The preset to perform; whether it exists depends on the scene.
    pub preset: String,
    /// Travel distance in pixels; 40 by default.
    pub distance: f64,
    /// How long the motion lasts; 12 frames by default, or a spring's settle
    /// time when the curve is a spring.
    pub duration: Time,
    /// Easing curve; `ease-out` by default.
    pub curve: Curve,
    /// When the motion starts, from the start of the scene; frame 0 by default.
    pub start: Time,
    /// On a text layer, the parts that each make the motion about their own
    /// anchor (a `by` choice of a text animator), instead of the whole layer.
    pub parts: Option<&'static str>,
    /// How long each part waits after the one before it; none by default.
    pub stagger: Time,
    /// Which part goes first (an `order` choice of a text animator); `first`
    /// by default.
    pub order: &'static str,
}

impl Behaviour {
    /// The frames the motion starts and ends on at `fps` frames per second.
    pub fn frames(&self, fps: f64) -> (f64, f64) {
        let start = self.start.to_frames(fps);
        (start, start + self.duration.to_frames(fps))
    }

    /// The frames an exit starts and ends on at `fps` frames per second, for
    /// a layer whose time runs out at frame `end` of its own time: its start
    /// (`@`) is how long before that it ends.
    pub fn exit_frames(&self, end: f64, fps: f64) -> (f64, f64) {
        let last = end - self.start.to_frames(fps);
        (last - self.duration.to_frames(fps), last)
    }

    /// The behaviour of `preset`, each token's field set over the defaults.
    fn tuned(preset: String, tokens: Vec<Token<'_>>) -> Behaviour {
        let defaults = Behaviour {
            preset,
            distance: 40.0,
            duration: Time::Frames(12.0),
            curve: Curve::named(DEFAULT_CURVE).expect("the default curve is in NAMED_CURVES"),
            start: Time::Frames(0.0),
            parts: None,
            stagger: Time::Frames(0.0),
            order: ORDERS[0],
        };
        let timed = tokens
            .iter()
            .any(|(tuning, _)| matches!(tuning, Tuning::Duration(_)));
        let behaviour = tokens
            .into_iter()
            .fold(defaults, |behaviour, (tuning, _)| tuning.apply(behaviour));
        // A spring without a duration lasts until it comes to rest.
        match behaviour.curve {
            Curve::Spring(spring) if !timed => Behaviour {
                duration: spring.settle(),
                ..behaviour
            },
            _ => behaviour,
        }
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
    /// A stagger or an order is given without the parts it orders.
    NoParts(&'s str),
    /// `stagger` is not followed by a time.
    NoStaggerTime,
    /// Two tokens set the same field: the first and the second.
    Twice(Field, &'s str, &'s str),
    /// The curve token is not a curve, with its own diagnostic.
    Curve(Diagnostic),
}

impl Problem<'_> {
    fn diagnostic(self) -> Diagnostic {
        let (kind, message) = match self {
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
            Problem::NoStaggerTime => (
                Kind::Syntax,
                "`stagger` needs the time each part waits, such as `stagger 3f`".to_owned(),
            ),
            Problem::NoParts(token) => (
                Kind::Missing,
                format!(
                    "`{token}` orders the parts of a text, so it needs them: add `char`, \
                     `char-no-space`, `word` or `line`"
                ),
            ),
            Problem::Twice(field, first, second) => (
                Kind::Conflict,
                format!("{} given twice: `{first}` and `{second}`", field.name()),
            ),
        };
        Diagnostic::new(kind, message).expected(EXPECTED)
    }
}

/// The field of [`Behaviour`] a tuning token sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Distance,
    Duration,
    Curve,
    Start,
    Parts,
    Stagger,
    Order,
}

impl Field {
    /// As in "duration given twice".
    fn name(self) -> &'static str {
        match self {
            Field::Distance => "distance",
            Field::Duration => "duration",
            Field::Curve => "curve",
            Field::Start => "start",
            Field::Parts => "parts",
            Field::Stagger => "stagger",
            Field::Order => "order",
        }
    }

    /// As in "`@12` is not a valid start time".
    fn noun(self) -> &'static str {
        match self {
            Field::Start => "start time",
            Field::Stagger => "stagger time",
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
    Parts(&'static str),
    Stagger(Time),
    Order(&'static str),
}

impl Tuning {
    fn field(self) -> Field {
        match self {
            Tuning::Distance(_) => Field::Distance,
            Tuning::Duration(_) => Field::Duration,
            Tuning::Curve(_) => Field::Curve,
            Tuning::Start(_) => Field::Start,
            Tuning::Parts(_) => Field::Parts,
            Tuning::Stagger(_) => Field::Stagger,
            Tuning::Order(_) => Field::Order,
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
            Tuning::Parts(parts) => Behaviour {
                parts: Some(parts),
                ..behaviour
            },
            Tuning::Stagger(stagger) => Behaviour {
                stagger,
                ..behaviour
            },
            Tuning::Order(order) => Behaviour { order, ..behaviour },
        }
    }
}

fn behaviour<'s>(input: &mut &'s str) -> Parsed<Behaviour, Problem<'s>> {
    delimited(
        blank0,
        (
            preset,
            repeat(0.., preceded(blank1, alt((stagger, token))))
                .try_fold(Vec::new, once_per_field)
                .try_map(ordered_parts),
        ),
        blank0,
    )
    .map(|(preset, tokens)| Behaviour::tuned(preset.to_owned(), tokens))
    .parse_next(input)
}

/// The tokens, unless a stagger or an order comes without the parts.
fn ordered_parts(tokens: Vec<Token<'_>>) -> std::result::Result<Vec<Token<'_>>, Problem<'_>> {
    let field = |f| tokens.iter().find(|(tuning, _)| tuning.field() == f);
    match (
        field(Field::Parts),
        field(Field::Stagger).or(field(Field::Order)),
    ) {
        (None, Some((_, text))) => Err(Problem::NoParts(text)),
        _ => Ok(tokens),
    }
}

/// `stagger <time>`, two words: how long each part waits.
fn stagger<'s>(input: &mut &'s str) -> Parsed<Token<'s>, Problem<'s>> {
    preceded(
        (STAGGER, peek(alt((blank1, eof)))),
        cut_err(
            preceded(blank1, word)
                .context(Problem::NoStaggerTime)
                .try_map(|given: &'s str| {
                    whole(time)
                        .parse(given)
                        .map_err(|_| Problem::Malformed(Field::Stagger, given))
                }),
        ),
    )
    .map(Tuning::Stagger)
    .with_taken()
    .parse_next(input)
}

/// The preset name; only its shape is checked, since presets depend on the scene.
fn preset<'s>(input: &mut &'s str) -> Parsed<&'s str, Problem<'s>> {
    word.try_map(|name| {
        Id::is_valid(name)
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
/// ends a distance, no digits (or `cubic(`, or a call such as `out-back(2)`
/// or `steps(4)`) make a curve and `f` or `s` ends a duration. Each shape commits, so a malformed token is reported as the
/// kind it looks like, and a misspelt curve gets the curve error.
fn tuning<'s>(input: &mut &'s str) -> Parsed<Tuning, Problem<'s>> {
    let token = *input;
    let malformed = |field| Problem::Malformed(field, token);
    alt((
        preceded('@', whole(time).context(malformed(Field::Start))).map(Tuning::Start),
        rest.verify_map(|w: &str| PARTS.iter().find(|p| **p == w).copied())
            .map(Tuning::Parts),
        rest.verify_map(|w: &str| ORDERS.iter().find(|o| **o == w).copied())
            .map(Tuning::Order),
        preceded(
            peek(ends_with("px")),
            whole(terminated(number, "px")).context(malformed(Field::Distance)),
        )
        .map(Tuning::Distance),
        preceded(
            peek(alt((CUBIC_PREFIX.void(), digit_free, called))),
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

/// A token shaped like a call, `name(…)`: a curve with parameters.
fn called<'s>(input: &mut &'s str) -> Parsed<(), Problem<'s>> {
    rest.verify(|token: &str| token.contains('(') && token.ends_with(')'))
        .void()
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
