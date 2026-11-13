//! The `at` shorthand: where a layer's centre sits, as `"<x> <y>"`.
//!
//! Part of step 1 (Read): parses a layer's `at` string into a [`Position`].
//! Step 2 resolves `center` against the canvas.

use std::fmt;

use crate::{ShorthandError, fmt_num};

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

impl Coord {
    fn parse(token: &str) -> Option<Coord> {
        if token == "center" {
            return Some(Coord::Center);
        }
        let value: f64 = token.parse().ok()?;
        value.is_finite().then_some(Coord::Px(value))
    }
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

impl Position {
    /// Parses exactly two whitespace-separated tokens, each a finite number
    /// of pixels or `center`.
    pub fn parse(text: &str) -> Result<Position, ShorthandError> {
        let bad = || ShorthandError::new(format!("`{text}` is not a valid position"), EXPECTED);
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let [x, y] = tokens[..] else {
            return Err(bad());
        };
        Ok(Position {
            x: Coord::parse(x).ok_or_else(bad)?,
            y: Coord::parse(y).ok_or_else(bad)?,
        })
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.x, self.y)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/position.rs"]
mod tests;
