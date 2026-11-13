//! Typed units parsed from shorthand strings: time, colour and size.
//!
//! Part of step 1 (Read). Scenes write `"0.4s"`, `"#0B0D12"` or `"1080p"`;
//! these types are what the rest of the pipeline works with.

use std::convert::Infallible;
use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use winnow::ascii::digit1;
use winnow::combinator::{alt, opt, preceded, separated_pair};
use winnow::error::ErrMode;
use winnow::prelude::*;
use winnow::stream::AsChar;
use winnow::token::take_while;

use crate::shorthand::lex::{self, Failure, Parsed};

pub(crate) const TIME_EXPECTED: &str = "`<number>f` or `<number>s`, e.g. `12f` or `0.4s`";
pub(crate) const COLOR_EXPECTED: &str = "`#RGB`, `#RRGGBB` or `#RRGGBBAA`";
pub(crate) const SIZE_EXPECTED: &str =
    "`720p`, `1080p`, `4k`, `square`, `vertical` or `<width>x<height>`";

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

/// A frame size in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width in pixels, always above zero.
    pub width: u32,
    /// Height in pixels, always above zero.
    pub height: u32,
}

impl FromStr for Time {
    type Err = Diagnostic;

    /// Parses `<number>f` or `<number>s`. The number must be finite and not
    /// negative, and the unit suffix is required.
    fn from_str(text: &str) -> Result<Time> {
        lex::parse_all(text, lex::time::<Infallible>, |_| {
            Diagnostic::syntax(format!("`{text}` is not a valid time")).expected(TIME_EXPECTED)
        })
    }
}

impl FromStr for Color {
    type Err = Diagnostic;

    /// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA`. Without an alpha digit pair
    /// the colour is opaque; `#RGB` doubles each digit (`#F80` is `#FF8800`).
    fn from_str(text: &str) -> Result<Color> {
        lex::parse_all(text, hex_color, |_| {
            Diagnostic::syntax(format!("`{text}` is not a valid colour")).expected(COLOR_EXPECTED)
        })
    }
}

impl FromStr for Size {
    type Err = Diagnostic;

    /// Parses a preset (`720p`, `1080p`, `4k`, `square`, `vertical`) or an
    /// explicit `<width>x<height>` where both sides are above zero.
    fn from_str(text: &str) -> Result<Size> {
        lex::parse_all(text, size, |_| {
            Diagnostic::syntax(format!("`{text}` is not a valid size")).expected(SIZE_EXPECTED)
        })
    }
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

/// A named size or `<width>x<height>`.
fn size(input: &mut &str) -> Parsed<Size> {
    alt((
        "720p".value(Size {
            width: 1280,
            height: 720,
        }),
        "1080p".value(Size {
            width: 1920,
            height: 1080,
        }),
        "4k".value(Size {
            width: 3840,
            height: 2160,
        }),
        "square".value(Size {
            width: 1080,
            height: 1080,
        }),
        "vertical".value(Size {
            width: 1080,
            height: 1920,
        }),
        separated_pair(pixels, 'x', pixels).map(|(width, height)| Size { width, height }),
    ))
    .parse_next(input)
}

/// A whole number of pixels above zero, as `u32` reads it: an optional `+`,
/// then digits.
fn pixels(input: &mut &str) -> Parsed<u32> {
    preceded(opt('+'), digit1)
        .parse_to()
        .verify(|n: &u32| *n > 0)
        .parse_next(input)
}

#[cfg(test)]
#[path = "../tests/unit/units.rs"]
mod tests;
