//! The `rect` shorthand, `"300x400 r24"`: a rectangle's size and corner radius.

use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, number, parse_whole};
use crate::{Diagnostic, Result};
use winnow::combinator::{delimited, opt, preceded, separated_pair};
use winnow::prelude::*;

use crate::doc::Form;

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

    /// Parses `<width>x<height> [r<radius>]`; the radius defaults to 0.
    fn from_str(text: &str) -> Result<RectShape> {
        parse_whole(text, rect, "rectangle", EXPECTED)
    }
}

impl Form for RectShape {
    const FORM: &str = EXPECTED;
}

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

fn side(input: &mut &str) -> Parsed<f64> {
    number.verify(|v: &f64| *v > 0.0).parse_next(input)
}

fn radius(input: &mut &str) -> Parsed<f64> {
    preceded('r', number.verify(|v: &f64| *v >= 0.0)).parse_next(input)
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/rect.rs"]
mod tests;
