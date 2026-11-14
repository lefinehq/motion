//! Typed units parsed from shorthand strings: time, colour and size.
//!
//! Part of step 1 (Read). Scenes write `"0.4s"`, `"#0B0D12"` or `"1080p"`;
//! these types are what the rest of the pipeline works with.

use std::fmt;
use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use motion_grammar::{Failure, Parsed, number, parse_all, parse_whole};
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
/// The accepted forms of a size, for a value that is not a string. The
/// names are listed by the unknown-size error, which reads [`SIZES`].
const SIZE_EXPECTED: &str = "a size name such as `1080p`, or `<width>x<height>`";
/// The form of an explicit size, as listed after the size names.
const EXPLICIT_FORM: &str = "<width>x<height>";
/// The expected form of an explicit size.
const EXPLICIT_EXPECTED: &str =
    "`<width>x<height>`, each a whole number of pixels above zero, e.g. `1920x1080`";

/// A point or length in time, in the unit the author wrote it in.
///
/// Time is kept as written until the frame rate is known; call
/// [`Time::to_frames`] to get the frames the pipeline works in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Time {
    /// A number of frames, written `12f`.
    Frames(f64),
    /// A number of seconds, written `0.4s`.
    Seconds(f64),
}

impl Time {
    /// Converts to frames at `fps` frames per second. Frame counts are
    /// returned unchanged; seconds are multiplied by `fps`.
    pub fn to_frames(self, fps: f64) -> f64 {
        match self {
            Time::Frames(f) => f,
            Time::Seconds(s) => s * fps,
        }
    }
}

impl FromStr for Time {
    type Err = Diagnostic;

    /// Parses `<number>f` or `<number>s`. The number must be finite and not
    /// negative, and the unit suffix is required.
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

/// `<number>f` or `<number>s`: a time in frames or seconds, not negative.
/// Also a part of the behaviour and anim grammars.
pub(crate) fn time<R>(input: &mut &str) -> Parsed<Time, R> {
    (amount, unit)
        .map(|(value, unit)| unit(value))
        .parse_next(input)
}

/// The number of a time: finite and not negative.
pub(crate) fn amount<R>(input: &mut &str) -> Parsed<f64, R> {
    number.verify(|v: &f64| *v >= 0.0).parse_next(input)
}

/// The unit of a time, `f` or `s`, as the variant it builds. The anim range
/// reads it apart from the number, because a bare start borrows the end's.
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
    /// Red, 0 to 255.
    pub r: u8,
    /// Green, 0 to 255.
    pub g: u8,
    /// Blue, 0 to 255.
    pub b: u8,
    /// Alpha, 0 (transparent) to 255 (opaque).
    pub a: u8,
}

impl FromStr for Color {
    type Err = Diagnostic;

    /// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA`. Without an alpha digit pair
    /// the colour is opaque; `#RGB` doubles each digit (`#F80` is `#FF8800`).
    fn from_str(text: &str) -> Result<Color> {
        parse_whole(text, hex_color, "colour", COLOR_EXPECTED)
    }
}

impl Form for Color {
    const FORM: &str = COLOR_EXPECTED;
}

/// `#` and three, six or eight hex digits. The longest form is tried first,
/// so `#RRGGBB` is not read as `#RGB` with digits left over.
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

/// Two hex digits: one channel of `#RRGGBB`.
fn channel(input: &mut &str) -> Parsed<u8> {
    hex_digits(2).parse_next(input)
}

/// One hex digit, doubled: one channel of `#RGB`, where `F` means `FF`.
fn short_channel(input: &mut &str) -> Parsed<u8> {
    hex_digits(1).map(|n| n * 17).parse_next(input)
}

/// Exactly `count` hex digits, read as one byte.
fn hex_digits<'s>(count: usize) -> impl Parser<&'s str, u8, ErrMode<Failure>> {
    take_while(count, AsChar::is_hex_digit).verify_map(|digits| u8::from_str_radix(digits, 16).ok())
}

/// A frame size in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width in pixels, always above zero.
    pub width: u32,
    /// Height in pixels, always above zero.
    pub height: u32,
}

/// The sizes that have a name, the only place size names are listed.
const SIZES: &[(&str, Size)] = &[
    ("720p", Size::new(1280, 720)),
    ("1080p", Size::new(1920, 1080)),
    ("4k", Size::new(3840, 2160)),
    ("square", Size::new(1080, 1080)),
    ("vertical", Size::new(1080, 1920)),
];

impl Size {
    /// A size of `width` by `height` pixels.
    const fn new(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    /// Looks a size up in [`SIZES`] by name.
    fn named(name: &str) -> Option<Size> {
        table::lookup(SIZES, name).map(|(_, size)| *size)
    }
}

impl FromStr for Size {
    type Err = Diagnostic;

    /// Parses a size name (`1080p`, `4k`, `square`, …) or an explicit
    /// `<width>x<height>` where both sides are above zero.
    fn from_str(text: &str) -> Result<Size> {
        parse_all(text, size, |problem| {
            // An explicit size names its problem once `<digits>x` is read,
            // so a failure without one is a name that is not in the table.
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
    /// The text is not a name in [`SIZES`], nor shaped like an explicit
    /// size.
    Unknown,
    /// The text starts like `<width>x`, but is not two whole numbers above
    /// zero.
    Explicit,
    /// The text starts with a valid `<width>x<height>`, then goes on; this
    /// is the text after the size.
    Trailing(&'s str),
}

impl SizeProblem<'_> {
    /// The diagnostic for `text`, which has this problem: the one place the
    /// size grammar's failures become messages.
    fn diagnostic(self, text: &str) -> Diagnostic {
        match self {
            SizeProblem::Unknown => {
                Diagnostic::unknown("size", text, table::names(SIZES)).or_expected(EXPLICIT_FORM)
            }
            SizeProblem::Explicit => Diagnostic::syntax(format!("`{text}` is not a valid size"))
                .expected(EXPLICIT_EXPECTED),
            SizeProblem::Trailing(after) => {
                // `after` is the end of `text`, so what comes before it is
                // the size as the author wrote it.
                let size = &text[..text.len() - after.len()];
                Diagnostic::syntax(format!(
                    "unexpected text after the size `{size}` in `{text}`"
                ))
                .expected(EXPLICIT_EXPECTED)
            }
        }
    }
}

/// A name from [`SIZES`] or `<width>x<height>`. Once `<digits>x` is read
/// the text is an explicit size, so its error gives that form instead of
/// listing the names; text after a valid size is named apart, since the
/// size itself is right (`640x360 `).
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

/// A whole number of pixels above zero, as `u32` reads it: an optional `+`,
/// then digits.
fn pixels<R>(input: &mut &str) -> Parsed<u32, R> {
    preceded(opt('+'), digit1)
        .parse_to()
        .verify(|n: &u32| *n > 0)
        .parse_next(input)
}

#[cfg(test)]
#[path = "../tests/unit/units.rs"]
mod tests;
