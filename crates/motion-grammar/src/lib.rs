//! `motion-grammar`: the `winnow` pieces every text grammar of motion shares,
//! and the one way a grammar's failure becomes a [`Diagnostic`].
//!
//! An internal crate under step 1 (Read) and the edit side branch. The
//! shorthands of `motion-format` (units, curves, behaviours, tracks) and the
//! edit line of `motion-edit` are each a small `winnow` grammar built from
//! these pieces, run through [`parse_all`] or [`parse_whole`], so every
//! grammar reads numbers, words and spaces the same way and maps its
//! failures in a single place.
//!
//! [`Failure`] is not an error anyone sees: it only lives inside a grammar,
//! and [`parse_all`] turns it into the one error type, [`Diagnostic`]. The
//! public crates built on this one keep `winnow` out of their own APIs.

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
/// `R` is the grammar's own list of reasons. A grammar gives one with
/// [`Parser::context`] on a part that commits, or raises one from a check
/// with [`Parser::try_map`] (through [`FromExternalError`]); a reason may
/// carry text, or a nested shorthand's diagnostic. Grammars that need no
/// reason leave `R` [`Infallible`].
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

/// The result of a grammar piece that fails with reasons of type `R`.
pub type Parsed<T, R = Infallible> = ModalResult<T, Failure<R>>;

/// Runs `grammar` over the whole of `text` and hands a failure's reason to
/// `explain`, each grammar's one error mapping.
///
/// # Errors
/// The [`Diagnostic`] `explain` makes when `grammar` fails or leaves text
/// unread.
pub fn parse_all<'s, T, R>(
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
///
/// # Errors
/// A syntax [`Diagnostic`] when `grammar` does not read all of `text`.
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

/// A finite decimal number, such as `12`, `-0.5`, `.2` or `1e3`.
///
/// # Errors
/// Backtracks on anything else, including `inf`, `NaN` and numbers too
/// large for an `f64`.
pub fn number<R>(input: &mut &str) -> Parsed<f64, R> {
    float.verify(|v: &f64| v.is_finite()).parse_next(input)
}

/// A run of non-whitespace: one token of a whitespace-separated grammar.
///
/// # Errors
/// Backtracks at whitespace or the end of the input.
pub fn word<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_till(1.., char::is_whitespace).parse_next(input)
}

/// Whitespace between tokens: at least one character.
///
/// # Errors
/// Backtracks when the input does not start with whitespace.
pub fn blank1<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_while(1.., char::is_whitespace).parse_next(input)
}

/// Optional whitespace, as around a shorthand or inside `cubic(…)`.
///
/// # Errors
/// Never fails.
pub fn blank0<'s, R>(input: &mut &'s str) -> Parsed<&'s str, R> {
    take_while(0.., char::is_whitespace).parse_next(input)
}

/// Text that ends with `suffix`, whatever comes before it: `12px` ends with
/// `px`. Consumes all of it and returns what `suffix` matched, so a caller
/// can name it: the `+` of `delay+`.
///
/// # Errors
/// Backtracks when no position of the input starts a `suffix` that reaches
/// its end.
pub fn ends_with<'s, O, R>(
    suffix: impl Parser<&'s str, O, ErrMode<Failure<R>>>,
) -> impl Parser<&'s str, O, ErrMode<Failure<R>>> {
    repeat_till(0.., any, (suffix, eof)).map(|((), (matched, _))| matched)
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
