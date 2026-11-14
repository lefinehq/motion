//! Typed units parsed from shorthand strings: time (`"0.4s"`), colour
//! (`"#0B0D12"`) and size (`"1080p"`).

use std::fmt;
use std::str::FromStr;

use crate::grammar::{Failure, Parsed, number, parse_all, parse_whole};
use crate::{Diagnostic, Result};
use winnow::ascii::digit1;
use winnow::combinator::{alt, cut_err, opt, peek, preceded, separated_pair};
use winnow::error::ErrMode;
use winnow::prelude::*;
use winnow::stream::AsChar;
use winnow::token::{rest, take_while};

use crate::doc::Form;
use crate::fmt_num;
use crate::table;

pub(crate) const TIME_EXPECTED: &str = "`<number>f` or `<number>s`, e.g. `12f` or `0.4s`";
pub(crate) const COLOR_EXPECTED: &str = "`#RGB`, `#RRGGBB` or `#RRGGBBAA`";
/// The accepted forms of a size, for a value that is not a string.
const SIZE_EXPECTED: &str = "a size name such as `1080p`, or `<width>x<height>`";
/// The explicit form, as listed after the size names.
const EXPLICIT_FORM: &str = "<width>x<height>";
const EXPLICIT_EXPECTED: &str =
    "`<width>x<height>`, each a whole number of pixels above zero, e.g. `1920x1080`";

/// A point or length in time, kept in the unit the author wrote until the
/// frame rate is known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Time {
    /// A number of frames, written `12f`.
    Frames(f64),
    /// A number of seconds, written `0.4s`.
    Seconds(f64),
}

impl Time {
    /// The time in frames at `fps` frames per second.
    pub fn to_frames(self, fps: f64) -> f64 {
        match self {
            Time::Frames(f) => f,
            Time::Seconds(s) => s * fps,
        }
    }
}

impl FromStr for Time {
    type Err = Diagnostic;

    /// Parses `<number>f` or `<number>s`, with a finite number of 0 or more.
    fn from_str(text: &str) -> Result<Time> {
        parse_whole(text, time, "time", TIME_EXPECTED)
    }
}

impl Form for Time {
    const FORM: &str = TIME_EXPECTED;
}

impl fmt::Display for Time {
    /// Writes the time as a scene would: `12f` or `0.4s`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Time::Frames(frames) => write!(f, "{}f", fmt_num(*frames)),
            Time::Seconds(seconds) => write!(f, "{}s", fmt_num(*seconds)),
        }
    }
}

/// `<number>f` or `<number>s`, also a piece of the behaviour and anim grammars.
pub(crate) fn time<R>(input: &mut &str) -> Parsed<Time, R> {
    (amount, unit)
        .map(|(value, unit)| unit(value))
        .parse_next(input)
}

/// The number of a time: finite and not negative.
pub(crate) fn amount<R>(input: &mut &str) -> Parsed<f64, R> {
    number.verify(|v: &f64| *v >= 0.0).parse_next(input)
}

/// The unit of a time, as the variant it builds; separate from the number
/// because a range's bare start borrows the end's unit.
pub(crate) fn unit<R>(input: &mut &str) -> Parsed<fn(f64) -> Time, R> {
    alt((
        'f'.value(Time::Frames as fn(f64) -> Time),
        's'.value(Time::Seconds as fn(f64) -> Time),
    ))
    .parse_next(input)
}

/// An 8-bit-per-channel RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha, 0 (transparent) to 255 (opaque).
    pub a: u8,
}

impl FromStr for Color {
    type Err = Diagnostic;

    /// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA`; `#F80` is `#FF8800`, opaque.
    fn from_str(text: &str) -> Result<Color> {
        parse_whole(text, hex_color, "colour", COLOR_EXPECTED)
    }
}

impl Form for Color {
    const FORM: &str = COLOR_EXPECTED;
}

/// `#` and eight, six or three hex digits, longest first so `#RRGGBB` is
/// not read as `#RGB` with digits left over.
fn hex_color(input: &mut &str) -> Parsed<Color> {
    preceded(
        '#',
        alt((
            (channel, channel, channel, channel).map(|(r, g, b, a)| Color { r, g, b, a }),
            (channel, channel, channel).map(|(r, g, b)| Color { r, g, b, a: 255 }),
            (short_channel, short_channel, short_channel).map(|(r, g, b)| Color {
                r,
                g,
                b,
                a: 255,
            }),
        )),
    )
    .parse_next(input)
}

fn channel(input: &mut &str) -> Parsed<u8> {
    hex_digits(2).parse_next(input)
}

/// One hex digit, doubled: `F` means `FF`.
fn short_channel(input: &mut &str) -> Parsed<u8> {
    hex_digits(1).map(|n| n * 17).parse_next(input)
}

fn hex_digits<'s>(count: usize) -> impl Parser<&'s str, u8, ErrMode<Failure>> {
    take_while(count, AsChar::is_hex_digit).verify_map(|digits| u8::from_str_radix(digits, 16).ok())
}

/// A frame size in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width in pixels, above zero.
    pub width: u32,
    /// Height in pixels, above zero.
    pub height: u32,
}

/// The sizes that have a name.
const SIZES: &[(&str, Size)] = &[
    ("720p", Size::new(1280, 720)),
    ("1080p", Size::new(1920, 1080)),
    ("4k", Size::new(3840, 2160)),
    ("square", Size::new(1080, 1080)),
    ("vertical", Size::new(1080, 1920)),
];

impl Size {
    const fn new(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    fn named(name: &str) -> Option<Size> {
        table::lookup(SIZES, name).map(|(_, size)| *size)
    }
}

impl FromStr for Size {
    type Err = Diagnostic;

    /// Parses a name from the size table or `<width>x<height>`, both above zero.
    fn from_str(text: &str) -> Result<Size> {
        parse_all(text, size, |problem| {
            problem.unwrap_or(SizeProblem::Unknown).diagnostic(text)
        })
    }
}

impl Form for Size {
    const FORM: &str = SIZE_EXPECTED;
}

/// What is wrong with a size string.
#[derive(Debug, Clone, Copy)]
enum SizeProblem<'s> {
    Unknown,
    /// Starts like `<width>x` but is not two whole numbers above zero.
    Explicit,
    /// A valid `<width>x<height>`, then this text.
    Trailing(&'s str),
}

impl SizeProblem<'_> {
    fn diagnostic(self, text: &str) -> Diagnostic {
        let message = match self {
            SizeProblem::Unknown => {
                return Diagnostic::unknown("size", text, table::names(SIZES))
                    .or_expected(EXPLICIT_FORM);
            }
            SizeProblem::Explicit => format!("`{text}` is not a valid size"),
            SizeProblem::Trailing(after) => {
                let size = &text[..text.len() - after.len()];
                format!("unexpected text after the size `{size}` in `{text}`")
            }
        };
        Diagnostic::syntax(message).expected(EXPLICIT_EXPECTED)
    }
}

/// A size name or `<width>x<height>`. Once `<digits>x` is read the text is
/// explicit, so its error gives that form rather than the names.
fn size<'s>(input: &mut &'s str) -> Parsed<Size, SizeProblem<'s>> {
    alt((
        rest.verify_map(Size::named),
        preceded(
            peek((opt('+'), digit1, 'x')),
            cut_err((
                separated_pair(pixels, 'x', pixels).context(SizeProblem::Explicit),
                rest.try_map(|after: &'s str| match after {
                    "" => Ok(()),
                    _ => Err(SizeProblem::Trailing(after)),
                }),
            )),
        )
        .map(|((width, height), ())| Size { width, height }),
    ))
    .parse_next(input)
}

/// A whole number of pixels above zero, as `u32` reads it (an optional `+`).
fn pixels<R>(input: &mut &str) -> Parsed<u32, R> {
    preceded(opt('+'), digit1)
        .parse_to()
        .verify(|n: &u32| *n > 0)
        .parse_next(input)
}

#[cfg(test)]
#[path = "../tests/unit/units.rs"]
mod tests;
