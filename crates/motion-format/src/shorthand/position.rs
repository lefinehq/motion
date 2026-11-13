//! The `at` shorthand: where a layer's centre sits, as `"<x> <y>"`.
//!
//! Part of step 1 (Read): parses a layer's `at` string into a [`Position`].
//! Step 2 resolves `center` against the canvas.

use std::fmt;
use std::str::FromStr;

use motion_diagnostic::{Diagnostic, Result};
use winnow::combinator::{alt, delimited, separated_pair};
use winnow::prelude::*;

use crate::doc::Form;
use crate::fmt_num;
use crate::lex::{self, Parsed, blank0, blank1, number};

pub(crate) const EXPECTED: &str =
    "`<x> <y>`, each a number of pixels or `center`, e.g. `center 260`";

/// One axis of a [`Position`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coord {
    /// The middle of the frame on this axis, written `center`.
    Center,
    /// A distance in pixels from the frame's top-left corner.
    Px(f64),
}

impl fmt::Display for Coord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Coord::Center => f.write_str("center"),
            Coord::Px(v) => f.write_str(&fmt_num(*v)),
        }
    }
}

/// Where a layer's centre sits, written `"<x> <y>"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// Horizontal coordinate.
    pub x: Coord,
    /// Vertical coordinate.
    pub y: Coord,
}

impl FromStr for Position {
    type Err = Diagnostic;

    /// Parses exactly two whitespace-separated tokens, each a finite number
    /// of pixels or `center`.
    fn from_str(text: &str) -> Result<Position> {
        lex::parse_whole(text, position, "position", EXPECTED)
    }
}

impl Form for Position {
    const FORM: &str = EXPECTED;
}

/// `<x> <y>`, with any whitespace around and between them.
fn position(input: &mut &str) -> Parsed<Position> {
    delimited(blank0, separated_pair(coord, blank1, coord), blank0)
        .map(|(x, y)| Position { x, y })
        .parse_next(input)
}

/// One axis: `center` or a number of pixels.
fn coord(input: &mut &str) -> Parsed<Coord> {
    alt(("center".value(Coord::Center), number.map(Coord::Px))).parse_next(input)
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.x, self.y)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/position.rs"]
mod tests;
