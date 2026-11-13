//! The `rect` shorthand: a rectangle's size and corner radius.
//!
//! Part of step 1 (Read): parses a layer's `rect` string into a [`RectShape`].

use crate::ShorthandError;

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
    pub fn parse(text: &str) -> Result<RectShape, ShorthandError> {
        let bad = || ShorthandError::new(format!("`{text}` is not a valid rectangle"), EXPECTED);
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
mod tests {
    use super::*;
    #[test]
    fn rect_parses_size_and_optional_radius() {
        assert_eq!(
            RectShape::parse("300x400 r24").unwrap(),
            RectShape {
                width: 300.0,
                height: 400.0,
                radius: 24.0
            }
        );
        assert_eq!(RectShape::parse("240x8").unwrap().radius, 0.0);
        assert!(
            RectShape::parse("300")
                .unwrap_err()
                .to_string()
                .contains("`<width>x<height> [r<radius>]`, e.g. `300x400 r24`")
        );
        assert!(RectShape::parse("0x10").is_err());
    }
    #[test]
    fn rect_rejects_malformed_input() {
        for bad in [
            "",
            "10x",
            "x10",
            "10x-5",
            "10x10 24",
            "10x10 r-1",
            "10x10 r",
            "10x10 r1 r2",
            "axb",
        ] {
            assert!(RectShape::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }
    #[test]
    fn rect_accepts_fractions_and_zero_radius() {
        let r = RectShape::parse("10.5x2 r0").unwrap();
        assert_eq!((r.width, r.height, r.radius), (10.5, 2.0, 0.0));
    }
}
