//! The `winnow` pieces every text grammar shares, so numbers, words and spaces
//! read the same everywhere, and [`parse_all`], the one way a grammar's failure
//! becomes a [`Diagnostic`].

use std::convert::Infallible;

use crate::{Diagnostic, Result};
use winnow::ascii::float;
use winnow::combinator::{eof, repeat_till};
use winnow::error::{AddContext, ErrMode, FromExternalError, ParserError};
use winnow::prelude::*;
use winnow::stream::{Stream, StreamIsPartial};
use winnow::token::{any, take_till, take_while};

/// Why a grammar failed: the reason the failing part gave, if any.
///
/// `R` is the grammar's own reason enum, given with [`Parser::context`] or
/// raised from [`Parser::try_map`]; it never leaves the grammar.
#[derive(Debug)]
pub struct Failure<R = Infallible>(Option<R>);

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
    /// Keeps the innermost reason, which names the most precise part that failed.
    fn add_context(self, _input: &I, _start: &<I as Stream>::Checkpoint, reason: R) -> Self {
        Failure(self.0.or(Some(reason)))
    }
}

impl<I, R> FromExternalError<I, R> for Failure<R> {
    fn from_external_error(_input: &I, reason: R) -> Self {
        Failure(Some(reason))
    }
}

/// The result of a grammar piece that fails with reasons of type `R`.
pub type Parsed<T, R = Infallible> = ModalResult<T, Failure<R>>;

/// Runs `grammar` over all of `input`; `explain` turns a failure's reason into a diagnostic.
///
/// # Errors
/// The [`Diagnostic`] `explain` makes when `grammar` fails or leaves input unread.
pub fn parse_all<I, T, R>(
    input: I,
    grammar: impl Parser<I, T, ErrMode<Failure<R>>>,
    explain: impl FnOnce(Option<R>) -> Diagnostic,
) -> Result<T>
where
    I: Stream + StreamIsPartial,
{
    parse_at(input, grammar, |reason, _| explain(reason))
}

/// [`parse_all`], also handing `explain` the offset where `grammar` failed:
/// for long inputs, such as binary-embedded text, where the offset is the
/// only way to find the fault.
///
/// # Errors
/// The [`Diagnostic`] `explain` makes when `grammar` fails or leaves input unread.
pub fn parse_at<I, T, R>(
    input: I,
    mut grammar: impl Parser<I, T, ErrMode<Failure<R>>>,
    explain: impl FnOnce(Option<R>, usize) -> Diagnostic,
) -> Result<T>
where
    I: Stream + StreamIsPartial,
{
    grammar.parse(input).map_err(|error| {
        let offset = error.offset();
        explain(error.into_inner().0, offset)
    })
}

/// [`parse_all`] for a grammar with one kind of failure: "`{text}` is not a valid {noun}".
///
/// # Errors
/// A syntax [`Diagnostic`] with `expected` as the form.
pub fn parse_whole<'s, T>(
    text: &'s str,
    grammar: impl Parser<&'s str, T, ErrMode<Failure>>,
    noun: &str,
    expected: &'static str,
) -> Result<T> {
    parse_all(text, grammar, |_| {
        Diagnostic::syntax(format!("`{text}` is not a valid {noun}")).expected(expected)
    })
}

/// A finite decimal number, such as `12`, `-0.5`, `.2` or `1e3`; never `inf` or `NaN`.
pub fn number<R>(input: &mut &str) -> Parsed<f64, R> {
    float.verify(|v: &f64| v.is_finite()).parse_next(input)
}

/// A run of non-whitespace: one token of a whitespace-separated grammar.
pub fn word<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_till(1.., char::is_whitespace).parse_next(input)
}

/// At least one whitespace character.
pub fn blank1<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_while(1.., char::is_whitespace).parse_next(input)
}

/// Optional whitespace.
pub fn blank0<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_while(0.., char::is_whitespace).parse_next(input)
}

/// All of the input, which must end with `suffix`; returns what `suffix` matched.
pub fn ends_with<'s, O, R>(
    suffix: impl Parser<&'s str, O, ErrMode<Failure<R>>>,
) -> impl Parser<&'s str, O, ErrMode<Failure<R>>> {
    repeat_till(0.., any, (suffix, eof)).map(|((), (matched, _))| matched)
}

#[cfg(test)]
#[path = "../tests/unit/grammar.rs"]
mod tests;
