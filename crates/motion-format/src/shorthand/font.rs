//! The `font` shorthand: family, size and weight in one string.
//!
//! Part of step 1 (Read): parses a text layer's `font` string into a [`Font`].

use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use winnow::ascii::float;
use winnow::combinator::{alt, eof, peek, preceded, repeat_till, terminated};
use winnow::prelude::*;

use super::lex::{self, Parsed, blank0, blank1, word};

pub(crate) const EXPECTED: &str = "`<family> <size> [<weight>]`, e.g. `Inter 96 700`";

/// A text font: family, size in pixels and weight.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// Family name; may contain spaces, e.g. `Noto Sans Arabic`.
    pub family: String,
    /// Size in pixels, above zero.
    pub size: f64,
    /// Weight from 100 to 900; 400 when not written.
    pub weight: u16,
}

const WEIGHT_RULE: &str = "weight must be 100, 200 \u{2026} 900";

/// The weight of a font that does not give one.
const DEFAULT_WEIGHT: u16 = 400;

impl FromStr for Font {
    type Err = Diagnostic;

    /// Parses `<family> <size> [<weight>]`.
    ///
    /// The family may contain spaces and may even end in a number, so the
    /// numbers are read from the end of the string:
    ///
    /// - one trailing number is the size, and the weight is 400:
    ///   `Noto Sans Arabic 40`;
    /// - two trailing numbers are `<size> <weight>` when the last is a valid
    ///   weight (a multiple of 100 from 100 to 900): `Inter 96 700`;
    /// - two trailing numbers whose last is not a valid weight are rejected
    ///   as ambiguous, because `Baloo 2 120` could be family `Baloo`, size 2,
    ///   weight 120, or family `Baloo 2` with size 120;
    /// - three trailing numbers are a family ending in a number, then the
    ///   size and weight: `Baloo 2 120 400`.
    fn from_str(text: &str) -> Result<Font> {
        let words = lex::parse_all(text, words, |_| {
            Diagnostic::syntax(format!("`{text}` has no size")).expected(EXPECTED)
        })?;
        words.into_font(text)
    }
}

/// A font string split by the grammar: the family words, then the numbers
/// that end the string, the last kept apart because there is always one.
struct Words<'s> {
    /// The words before the trailing numbers; the last one is not a number.
    family: Vec<&'s str>,
    /// The trailing numbers before the last, as written.
    numbers: Vec<Number<'s>>,
    /// The last number of the string.
    last: Number<'s>,
}

/// A number word: its text, for messages and the weight, and its value.
type Number<'s> = (&'s str, f64);

impl Words<'_> {
    /// Applies the rules of [`Font::from_str`] to the trailing numbers.
    fn into_font(self, text: &str) -> Result<Font> {
        let Words {
            family,
            numbers,
            last,
        } = self;
        // The family words end in a word that is not a number, so an empty
        // list means the family would be numbers alone, as in `96 700`.
        if family.is_empty() {
            return Err(
                Diagnostic::syntax(format!("`{text}` has no font family")).expected(EXPECTED)
            );
        }
        // `extra` holds the trailing numbers that end the family name.
        let (extra, (size_text, size), weight) = match (numbers.as_slice(), weight(last.0)) {
            ([], _) => (&[][..], last, DEFAULT_WEIGHT),
            ([extra @ .., size], Some(weight)) => (extra, *size, weight),
            ([size], None) => {
                let explicit = [family.join(" ").as_str(), size.0, last.0].join(" ");
                return Err(Diagnostic::syntax(format!(
                    "`{text}` is ambiguous: if the family name ends in a number, \
                     write the size and weight explicitly, e.g. `{explicit} 400`; \
                     otherwise the {WEIGHT_RULE}"
                ))
                .expected(EXPECTED));
            }
            (_, None) => {
                return Err(Diagnostic::syntax(format!(
                    "`{}` is not a valid weight; the {WEIGHT_RULE}",
                    last.0
                ))
                .expected(EXPECTED));
            }
        };
        if !(size.is_finite() && size > 0.0) {
            return Err(Diagnostic::syntax(format!(
                "`{size_text}` is not a valid size; it must be above zero"
            ))
            .expected(EXPECTED));
        }
        let family: Vec<&str> = family
            .into_iter()
            .chain(extra.iter().map(|(word, _)| *word))
            .collect();
        Ok(Font {
            family: family.join(" "),
            size,
            weight,
        })
    }
}

/// The weight `word` stands for, if it is one: a multiple of 100 from 100
/// to 900.
fn weight(word: &str) -> Option<u16> {
    word.parse::<u16>()
        .ok()
        .filter(|w| (100..=900).contains(w) && w % 100 == 0)
}

/// Family words, then the numbers that end the string. `repeat_till` tries
/// the trailing numbers before each word, so the family stops at the last
/// word that is not a number.
fn words<'s>(input: &mut &'s str) -> Parsed<Words<'s>> {
    preceded(
        blank0,
        repeat_till(0.., terminated(word, blank1), trailing_numbers),
    )
    .map(|(family, (numbers, last))| Words {
        family,
        numbers,
        last,
    })
    .parse_next(input)
}

/// The numbers that end a font string: numbers until the last one, which
/// ends the text.
fn trailing_numbers<'s>(input: &mut &'s str) -> Parsed<(Vec<Number<'s>>, Number<'s>)> {
    repeat_till(
        0..,
        terminated(number_word, blank1),
        terminated(number_word, (blank0, eof)),
    )
    .parse_next(input)
}

/// A whole word that is a number, with its text. Any number counts, even
/// `inf`: it still ends the family, and the size check then rejects it.
fn number_word<'s>(input: &mut &'s str) -> Parsed<Number<'s>> {
    terminated(float, peek(alt((blank1, eof))))
        .with_taken()
        .map(|(value, text)| (text, value))
        .parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/font.rs"]
mod tests;
