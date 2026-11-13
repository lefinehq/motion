//! The `font` shorthand: family, size and weight in one string.
//!
//! Part of step 1 (Read): parses a text layer's `font` string into a [`Font`].

use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use motion_grammar::{Parsed, blank0, blank1, parse_all, word};
use winnow::ascii::float;
use winnow::combinator::{alt, eof, peek, preceded, repeat_till, terminated};
use winnow::prelude::*;

use crate::doc::Form;

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
        parse_all(text, font, |problem| {
            // `Words::into_font` names every other problem, so a failure
            // without one means the grammar found no trailing number.
            let message = match problem.unwrap_or(Problem::NoSize) {
                Problem::NoSize => format!("`{text}` has no size"),
                Problem::NoFamily => format!("`{text}` has no font family"),
                Problem::Ambiguous { explicit } => format!(
                    "`{text}` is ambiguous: if the family name ends in a number, \
                     write the size and weight explicitly, e.g. `{explicit} 400`; \
                     otherwise the {WEIGHT_RULE}"
                ),
                Problem::Weight(weight) => {
                    format!("`{weight}` is not a valid weight; the {WEIGHT_RULE}")
                }
                Problem::Size(size) => {
                    format!("`{size}` is not a valid size; it must be above zero")
                }
            };
            Diagnostic::syntax(message).expected(EXPECTED)
        })
    }
}

impl Form for Font {
    const FORM: &str = EXPECTED;
}

/// What is wrong with a font string, with the text it is wrong about.
#[derive(Debug)]
enum Problem<'s> {
    /// The string does not end in a number.
    NoSize,
    /// The string has numbers only.
    NoFamily,
    /// Two trailing numbers whose last is not a weight; `explicit` is the
    /// string with the family ending in the first number.
    Ambiguous { explicit: String },
    /// The last of three or more trailing numbers is not a weight.
    Weight(&'s str),
    /// The size is not above zero.
    Size(&'s str),
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

impl<'s> Words<'s> {
    /// Applies the rules of [`Font::from_str`] to the trailing numbers.
    fn into_font(self) -> std::result::Result<Font, Problem<'s>> {
        let Words {
            mut family,
            numbers,
            last,
        } = self;
        // The family words end in a word that is not a number, so an empty
        // list means the family would be numbers alone, as in `96 700`.
        if family.is_empty() {
            return Err(Problem::NoFamily);
        }
        // `extra` holds the trailing numbers that end the family name.
        let (extra, (size_text, size), weight) = match (numbers.as_slice(), weight(last.0)) {
            (no_extra @ [], _) => (no_extra, last, DEFAULT_WEIGHT),
            ([extra @ .., size], Some(weight)) => (extra, *size, weight),
            ([size], None) => {
                let explicit = format!("{} {} {}", family.join(" "), size.0, last.0);
                return Err(Problem::Ambiguous { explicit });
            }
            (_, None) => return Err(Problem::Weight(last.0)),
        };
        if !(size.is_finite() && size > 0.0) {
            return Err(Problem::Size(size_text));
        }
        family.extend(extra.iter().map(|(word, _)| *word));
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

/// A font string, checked against the rules of [`Font::from_str`].
fn font<'s>(input: &mut &'s str) -> Parsed<Font, Problem<'s>> {
    words.try_map(Words::into_font).parse_next(input)
}

/// Family words, then the numbers that end the string. `repeat_till` tries
/// the trailing numbers before each word, so the family stops at the last
/// word that is not a number.
fn words<'s, R>(input: &mut &'s str) -> Parsed<Words<'s>, R> {
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
fn trailing_numbers<'s, R>(input: &mut &'s str) -> Parsed<(Vec<Number<'s>>, Number<'s>), R> {
    repeat_till(
        0..,
        terminated(number_word, blank1),
        terminated(number_word, (blank0, eof)),
    )
    .parse_next(input)
}

/// A whole word that is a number, with its text. Any number counts, even
/// `inf`: it still ends the family, and the size check then rejects it.
fn number_word<'s, R>(input: &mut &'s str) -> Parsed<Number<'s>, R> {
    terminated(float, peek(alt((blank1, eof))))
        .with_taken()
        .map(|(value, text)| (text, value))
        .parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/font.rs"]
mod tests;
