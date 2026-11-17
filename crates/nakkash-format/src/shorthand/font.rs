//! The `font` shorthand, `"Inter 96 700"`: family, size and weight.

use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, parse_all, word};
use crate::{Diagnostic, Result};
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
const DEFAULT_WEIGHT: u16 = 400;

impl FromStr for Font {
    type Err = Diagnostic;

    /// Parses `<family> <size> [<weight>]`, reading numbers from the end since
    /// a family may itself end in a number:
    ///
    /// - one trailing number is the size: `Noto Sans Arabic 40`;
    /// - two are `<size> <weight>` when the last is a weight: `Inter 96 700`,
    ///   and ambiguous otherwise: `Baloo 2 120`;
    /// - three end the family with a number: `Baloo 2 120 400`.
    fn from_str(text: &str) -> Result<Font> {
        parse_all(text, font, |problem| {
            problem.unwrap_or(Problem::NoSize).diagnostic(text)
        })
    }
}

impl Form for Font {
    const FORM: &str = EXPECTED;
}

/// What is wrong with a font string.
#[derive(Debug)]
enum Problem<'s> {
    NoSize,
    NoFamily,
    /// Two trailing numbers whose last is not a weight; `explicit` is the
    /// string with the family ending in the first.
    Ambiguous {
        explicit: String,
    },
    /// The last of three or more trailing numbers is not a weight.
    Weight(&'s str),
    /// The size is not above zero.
    Size(&'s str),
}

impl Problem<'_> {
    fn diagnostic(self, text: &str) -> Diagnostic {
        let message = match self {
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
            Problem::Size(size) => format!("`{size}` is not a valid size; it must be above zero"),
        };
        Diagnostic::syntax(message).expected(EXPECTED)
    }
}

/// A font string split by the grammar: the family words, then the trailing numbers.
struct Words<'s> {
    /// The words before the trailing numbers; the last is not a number.
    family: Vec<&'s str>,
    /// The trailing numbers before the last.
    numbers: Vec<Number<'s>>,
    last: Number<'s>,
}

/// A number word: its text and its value.
type Number<'s> = (&'s str, f64);

impl<'s> Words<'s> {
    /// Applies the rules of [`Font::from_str`] to the trailing numbers.
    fn into_font(self) -> std::result::Result<Font, Problem<'s>> {
        let Words {
            mut family,
            numbers,
            last,
        } = self;
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

/// The weight `word` stands for: a multiple of 100 from 100 to 900.
fn weight(word: &str) -> Option<u16> {
    word.parse::<u16>()
        .ok()
        .filter(|w| (100..=900).contains(w) && w % 100 == 0)
}

fn font<'s>(input: &mut &'s str) -> Parsed<Font, Problem<'s>> {
    words.try_map(Words::into_font).parse_next(input)
}

/// Family words, then the numbers that end the string; `repeat_till` tries
/// the numbers before each word, so the family stops at its last non-number.
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

fn trailing_numbers<'s, R>(input: &mut &'s str) -> Parsed<(Vec<Number<'s>>, Number<'s>), R> {
    repeat_till(
        0..,
        terminated(number_word, blank1),
        terminated(number_word, (blank0, eof)),
    )
    .parse_next(input)
}

/// A whole word that is a number. Even `inf` counts: it ends the family, and
/// the size check rejects it.
fn number_word<'s, R>(input: &mut &'s str) -> Parsed<Number<'s>, R> {
    terminated(float, peek(alt((blank1, eof))))
        .with_taken()
        .map(|(value, text)| (text, value))
        .parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/font.rs"]
mod tests;
