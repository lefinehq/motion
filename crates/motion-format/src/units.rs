//! Typed units parsed from shorthand strings: time, colour and size.
//!
//! Part of step 1 (Read). Scenes write `"0.4s"`, `"#0B0D12"` or `"1080p"`;
//! these types are what the rest of the pipeline works with.

use motion_diagnostic::{Diagnostic, Result};

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
    /// Parses `<number>f` or `<number>s`. The number must be finite and not
    /// negative, and the unit suffix is required.
    pub fn parse(text: &str) -> Result<Time> {
        let bad =
            || Diagnostic::syntax(format!("`{text}` is not a valid time")).expected(TIME_EXPECTED);
        let (number, is_frames) = if let Some(n) = text.strip_suffix('f') {
            (n, true)
        } else if let Some(n) = text.strip_suffix('s') {
            (n, false)
        } else {
            return Err(bad());
        };
        let value: f64 = number.parse().map_err(|_| bad())?;
        if !value.is_finite() || value < 0.0 {
            return Err(bad());
        }
        Ok(if is_frames {
            Time::Frames(value)
        } else {
            Time::Seconds(value)
        })
    }

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

impl Color {
    /// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA`. Without an alpha digit pair
    /// the colour is opaque; `#RGB` doubles each digit (`#F80` is `#FF8800`).
    pub fn parse(text: &str) -> Result<Color> {
        let bad = || {
            Diagnostic::syntax(format!("`{text}` is not a valid colour")).expected(COLOR_EXPECTED)
        };
        let digits = text.strip_prefix('#').ok_or_else(bad)?;
        // Checking ASCII first makes the byte-index slicing below safe.
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let pair = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).map_err(|_| bad());
        let nibble = |i: usize| {
            u8::from_str_radix(&digits[i..i + 1], 16)
                .map(|n| n * 17)
                .map_err(|_| bad())
        };
        match digits.len() {
            3 => Ok(Color {
                r: nibble(0)?,
                g: nibble(1)?,
                b: nibble(2)?,
                a: 255,
            }),
            6 => Ok(Color {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
                a: 255,
            }),
            8 => Ok(Color {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
                a: pair(6)?,
            }),
            _ => Err(bad()),
        }
    }
}

/// A frame size in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width in pixels, always above zero.
    pub width: u32,
    /// Height in pixels, always above zero.
    pub height: u32,
}

impl Size {
    /// Parses a preset (`720p`, `1080p`, `4k`, `square`, `vertical`) or an
    /// explicit `<width>x<height>` where both sides are above zero.
    pub fn parse(text: &str) -> Result<Size> {
        let (width, height) = match text {
            "720p" => (1280, 720),
            "1080p" => (1920, 1080),
            "4k" => (3840, 2160),
            "square" => (1080, 1080),
            "vertical" => (1080, 1920),
            _ => {
                let bad = || {
                    Diagnostic::syntax(format!("`{text}` is not a valid size"))
                        .expected(SIZE_EXPECTED)
                };
                let (w, h) = text.split_once('x').ok_or_else(bad)?;
                let w: u32 = w.parse().map_err(|_| bad())?;
                let h: u32 = h.parse().map_err(|_| bad())?;
                if w == 0 || h == 0 {
                    return Err(bad());
                }
                (w, h)
            }
        };
        Ok(Size { width, height })
    }
}

#[cfg(test)]
#[path = "../tests/unit/units.rs"]
mod tests;
