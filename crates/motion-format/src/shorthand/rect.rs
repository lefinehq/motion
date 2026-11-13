//! The `rect` shorthand: a rectangle's size and corner radius.
//!
//! Part of step 1 (Read): parses a layer's `rect` string into a [`RectShape`].

use motion_diagnostic::{Diagnostic, Result};

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

impl RectShape {
    /// Parses `<width>x<height>` with an optional `r<radius>` token. Width
    /// and height must be above zero; the radius defaults to 0.
    pub fn parse(text: &str) -> Result<RectShape> {
        let bad =
            || Diagnostic::syntax(format!("`{text}` is not a valid rectangle")).expected(EXPECTED);
        let number = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite());
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let (size, radius) = match tokens[..] {
            [size] => (size, None),
            [size, radius] => (size, Some(radius)),
            _ => return Err(bad()),
        };
        let (w, h) = size.split_once('x').ok_or_else(bad)?;
        let width = number(w).filter(|v| *v > 0.0).ok_or_else(bad)?;
        let height = number(h).filter(|v| *v > 0.0).ok_or_else(bad)?;
        let radius = match radius {
            None => 0.0,
            Some(r) => r
                .strip_prefix('r')
                .and_then(number)
                .filter(|v| *v >= 0.0)
                .ok_or_else(bad)?,
        };
        Ok(RectShape {
            width,
            height,
            radius,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/rect.rs"]
mod tests;
