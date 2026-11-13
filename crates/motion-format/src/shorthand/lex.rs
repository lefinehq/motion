//! The grammar pieces every shorthand shares: numbers, times, words and
//! whitespace, and the one way a grammar's failure becomes a [`Diagnostic`].
//!
//! Part of step 1 (Read). Each shorthand type implements `FromStr` with a
//! small `winnow` grammar built from these pieces, and runs it through
//! [`parse_all`], so every module maps its failures in a single place.

use std::convert::Infallible;

use motion_diagnostic::{Diagnostic, Result};
use winnow::ascii::float;
use winnow::combinator::{alt, eof, repeat_till};
use winnow::error::{AddContext, ErrMode, FromExternalError, ParserError};
use winnow::prelude::*;
use winnow::stream::Stream;
use winnow::token::{any, take_till, take_while};

use crate::units::Time;

/// Why a shorthand grammar failed.
///
/// `R` is the module's own list of reasons, attached with
/// [`Parser::context`] to the parts of a grammar that commit; modules that
/// need none leave it [`Infallible`].
#[derive(Debug)]
pub(crate) enum Failure<R = Infallible> {
    /// The text does not have the expected shape. The reason says which
    /// committed part failed; `None` leaves the message to the module.
    Syntax(Option<R>),
    /// A part read by another shorthand, such as the curve inside a
    /// behaviour, failed with its own diagnostic, which is passed on as is.
    Diagnosed(Diagnostic),
}

impl<I: Stream, R> ParserError<I> for Failure<R> {
    type Inner = Self;

    fn from_input(_input: &I) -> Self {
        Failure::Syntax(None)
    }

    fn into_inner(self) -> std::result::Result<Self, Self> {
        Ok(self)
    }
}

impl<I: Stream, R> AddContext<I, R> for Failure<R> {
    /// Keeps the innermost reason: it names the most precise part that
    /// failed, so an enclosing reason never replaces it.
    fn add_context(self, _input: &I, _start: &<I as Stream>::Checkpoint, reason: R) -> Self {
        match self {
            Failure::Syntax(None) => Failure::Syntax(Some(reason)),
            explained => explained,
        }
    }
}

impl<I, R> FromExternalError<I, Diagnostic> for Failure<R> {
    fn from_external_error(_input: &I, diagnostic: Diagnostic) -> Self {
        Failure::Diagnosed(diagnostic)
    }
}

/// The result of a shorthand grammar piece.
pub(crate) type Parsed<T, R = Infallible> = ModalResult<T, Failure<R>>;

/// Runs `grammar` over the whole of `text` and turns a failure into a
/// [`Diagnostic`]: a diagnosed failure is returned as it is, and a syntax
/// failure goes to `explain` with its reason. `explain` is each module's one
/// error mapping.
pub(crate) fn parse_all<'s, T, R>(
    text: &'s str,
    mut grammar: impl Parser<&'s str, T, ErrMode<Failure<R>>>,
    explain: impl FnOnce(Option<R>) -> Diagnostic,
) -> Result<T> {
    grammar
        .parse(text)
        .map_err(|error| match error.into_inner() {
            Failure::Syntax(reason) => explain(reason),
            Failure::Diagnosed(diagnostic) => diagnostic,
        })
}

/// A finite decimal number, such as `12`, `-0.5`, `.2` or `1e3`.
pub(crate) fn number<R>(input: &mut &str) -> Parsed<f64, R> {
    float.verify(|v: &f64| v.is_finite()).parse_next(input)
}

/// `<number>f` or `<number>s`: a time in frames or seconds, not negative.
pub(crate) fn time<R>(input: &mut &str) -> Parsed<Time, R> {
    let value = number.verify(|v: &f64| *v >= 0.0).parse_next(input)?;
    alt((
        'f'.value(Time::Frames(value)),
        's'.value(Time::Seconds(value)),
    ))
    .parse_next(input)
}

/// A run of non-whitespace: one token of a whitespace-separated shorthand.
pub(crate) fn word<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_till(1.., char::is_whitespace).parse_next(input)
}

/// Whitespace between tokens: at least one character.
pub(crate) fn blank1<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_while(1.., char::is_whitespace).parse_next(input)
}

/// Optional whitespace, as around a shorthand or inside `cubic(…)`.
pub(crate) fn blank0<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_while(0.., char::is_whitespace).parse_next(input)
}

/// Text that ends with `suffix`, whatever comes before it: `12px` ends with
/// `px`. Consumes all of it.
pub(crate) fn ends_with<'s, O, R>(
    suffix: impl Parser<&'s str, O, ErrMode<Failure<R>>>,
) -> impl Parser<&'s str, (), ErrMode<Failure<R>>> {
    repeat_till(0.., any, (suffix, eof)).map(|((), _)| ())
}
