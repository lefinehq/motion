//! The `rect` shorthand: a rectangle's size and corner radius.
//!
//! Part of step 1 (Read): parses a layer's `rect` string into a [`RectShape`].

use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use winnow::combinator::{delimited, opt, preceded, separated_pair};
use winnow::prelude::*;

use crate::doc::Form;
use crate::lex::{self, Parsed, blank0, blank1, number};

pub(crate) const EXPECTED: &str = "`<width>x<height> [r<radius>]`, e.g. `300x400 r24`";

/// A rectangle's size and corner radius, all in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectShape {
    /// Width, above zero.
    pub width: f64,
    /// Height, above zero.
    pub height: f64,
    /// Corner radius, zero for square corners.
    pub radius: f64,
}

impl FromStr for RectShape {
    type Err = Diagnostic;

    /// Parses `<width>x<height>` with an optional `r<radius>` token. Width
    /// and height must be above zero; the radius defaults to 0.
    fn from_str(text: &str) -> Result<RectShape> {
        lex::parse_whole(text, rect, "rectangle", EXPECTED)
    }
}

impl Form for RectShape {
    const FORM: &str = EXPECTED;
}

/// `<width>x<height> [r<radius>]`, with any whitespace around and between.
fn rect(input: &mut &str) -> Parsed<RectShape> {
    delimited(
        blank0,
        (
            separated_pair(side, 'x', side),
            opt(preceded(blank1, radius)),
        ),
        blank0,
    )
    .map(|((width, height), radius)| RectShape {
        width,
        height,
        radius: radius.unwrap_or(0.0),
    })
    .parse_next(input)
}

/// A width or height: a number above zero.
fn side(input: &mut &str) -> Parsed<f64> {
    number.verify(|v: &f64| *v > 0.0).parse_next(input)
}

/// `r<radius>`: a corner radius of zero or more.
fn radius(input: &mut &str) -> Parsed<f64> {
    preceded('r', number.verify(|v: &f64| *v >= 0.0)).parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/rect.rs"]
mod tests;
