//! The grammar pieces every shorthand shares: numbers, words and whitespace,
//! and the one way a grammar's failure becomes a [`Diagnostic`].
//!
//! Part of step 1 (Read). The units, the curves and the shorthands each
//! implement `FromStr` with a small `winnow` grammar built from these pieces,
//! and run it through [`parse_all`] or [`parse_whole`], so every module maps
//! its failures in a single place. This module depends on no other module of
//! the crate.

use std::convert::Infallible;

use motion_diagnostic::{Diagnostic, Result};
use winnow::ascii::float;
use winnow::combinator::{eof, repeat_till};
use winnow::error::{AddContext, ErrMode, FromExternalError, ParserError};
use winnow::prelude::*;
use winnow::stream::Stream;
use winnow::token::{any, take_till, take_while};

/// Why a grammar failed: the reason the failing part gave, if any.
///
/// `R` is the module's own list of reasons. A grammar gives one with
/// [`Parser::context`] on a part that commits, or raises one from a check
/// with [`Parser::try_map`] (through [`FromExternalError`]); a reason may
/// carry text, or a nested shorthand's diagnostic. Modules that need no
/// reason leave `R` [`Infallible`].
#[derive(Debug)]
pub(crate) struct Failure<R = Infallible>(Option<R>);

impl<I: Stream, R> ParserError<I> for Failure<R> {
    type Inner = Self;

    fn from_input(_input: &I) -> Self {
        Failure(None)
    }

    fn into_inner(self) -> std::result::Result<Self, Self> {
        Ok(self)
    }
}

impl<I: Stream, R> AddContext<I, R> for Failure<R> {
    /// Keeps the innermost reason: it names the most precise part that
    /// failed, so an enclosing reason never replaces it.
    fn add_context(self, _input: &I, _start: &<I as Stream>::Checkpoint, reason: R) -> Self {
        Failure(self.0.or(Some(reason)))
    }
}

impl<I, R> FromExternalError<I, R> for Failure<R> {
    fn from_external_error(_input: &I, reason: R) -> Self {
        Failure(Some(reason))
    }
}

/// The result of a shorthand grammar piece.
pub(crate) type Parsed<T, R = Infallible> = ModalResult<T, Failure<R>>;

/// Runs `grammar` over the whole of `text` and hands a failure's reason to
/// `explain`, each module's one error mapping.
pub(crate) fn parse_all<'s, T, R>(
    text: &'s str,
    mut grammar: impl Parser<&'s str, T, ErrMode<Failure<R>>>,
    explain: impl FnOnce(Option<R>) -> Diagnostic,
) -> Result<T> {
    grammar
        .parse(text)
        .map_err(|error| explain(error.into_inner().0))
}

/// [`parse_all`] for a grammar whose every failure means the same thing:
/// "`{text}` is not a valid {noun}", with `expected` as the form.
pub(crate) fn parse_whole<'s, T>(
    text: &'s str,
    grammar: impl Parser<&'s str, T, ErrMode<Failure>>,
    noun: &str,
    expected: &'static str,
) -> Result<T> {
    parse_all(text, grammar, |_| {
        Diagnostic::syntax(format!("`{text}` is not a valid {noun}")).expected(expected)
    })
}

/// A finite decimal number, such as `12`, `-0.5`, `.2` or `1e3`.
pub(crate) fn number<R>(input: &mut &str) -> Parsed<f64, R> {
    float.verify(|v: &f64| v.is_finite()).parse_next(input)
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
