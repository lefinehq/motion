//! Typed units parsed from shorthand strings: time, colour and size.
//!
//! Part of step 1 (Read). Scenes write `"0.4s"`, `"#0B0D12"` or `"1080p"`;
//! these types are what the rest of the pipeline works with.

use crate::ShorthandError;

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
    pub fn parse(text: &str) -> Result<Time, ShorthandError> {
        let bad = || ShorthandError::new(format!("`{text}` is not a valid time"), TIME_EXPECTED);
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
    pub fn parse(text: &str) -> Result<Color, ShorthandError> {
        let bad = || ShorthandError::new(format!("`{text}` is not a valid colour"), COLOR_EXPECTED);
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
    pub fn parse(text: &str) -> Result<Size, ShorthandError> {
        let (width, height) = match text {
            "720p" => (1280, 720),
            "1080p" => (1920, 1080),
            "4k" => (3840, 2160),
            "square" => (1080, 1080),
            "vertical" => (1080, 1920),
            _ => {
                let bad =
                    || ShorthandError::new(format!("`{text}` is not a valid size"), SIZE_EXPECTED);
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
mod tests {
    use super::*;

    #[test]
    fn time_parses_frames_and_seconds() {
        assert_eq!(Time::parse("12f").unwrap(), Time::Frames(12.0));
        assert_eq!(Time::parse("0.4s").unwrap(), Time::Seconds(0.4));
        assert_eq!(Time::parse("5s").unwrap().to_frames(30.0), 150.0);
        assert_eq!(Time::parse("12f").unwrap().to_frames(60.0), 12.0);
    }
    #[test]
    fn time_rejects_bad_input_with_expected_form() {
        let e = Time::parse("12").unwrap_err();
        assert!(
            e.to_string()
                .contains("expected `<number>f` or `<number>s`, e.g. `12f` or `0.4s`")
        );
        assert!(Time::parse("-3f").is_err());
        assert!(Time::parse("f").is_err());
    }
    #[test]
    fn color_parses_three_six_and_eight_digits() {
        assert_eq!(
            Color::parse("#FFF").unwrap(),
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            }
        );
        assert_eq!(
            Color::parse("#0B0D12").unwrap(),
            Color {
                r: 11,
                g: 13,
                b: 18,
                a: 255
            }
        );
        assert_eq!(Color::parse("#00000033").unwrap().a, 0x33);
        assert!(
            Color::parse("red")
                .unwrap_err()
                .to_string()
                .contains("`#RGB`, `#RRGGBB` or `#RRGGBBAA`")
        );
        assert!(Color::parse("#GGG").is_err());
    }
    #[test]
    fn size_parses_presets_and_explicit_pixels() {
        assert_eq!(
            Size::parse("1080p").unwrap(),
            Size {
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            Size::parse("720p").unwrap(),
            Size {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(
            Size::parse("4k").unwrap(),
            Size {
                width: 3840,
                height: 2160
            }
        );
        assert_eq!(
            Size::parse("square").unwrap(),
            Size {
                width: 1080,
                height: 1080
            }
        );
        assert_eq!(
            Size::parse("vertical").unwrap(),
            Size {
                width: 1080,
                height: 1920
            }
        );
        assert_eq!(
            Size::parse("640x360").unwrap(),
            Size {
                width: 640,
                height: 360
            }
        );
        assert!(Size::parse("0x360").is_err());
        assert!(
            Size::parse("big")
                .unwrap_err()
                .to_string()
                .contains("`720p`, `1080p`, `4k`, `square`, `vertical` or `<width>x<height>`")
        );
    }
    #[test]
    fn time_rejects_non_finite_and_malformed_numbers() {
        for bad in ["", "nanf", "inff", "infs", "1.2.3s", "1 s", "12F", "s"] {
            assert!(Time::parse(bad).is_err(), "{bad:?} should be rejected");
        }
        assert_eq!(Time::parse("0f").unwrap(), Time::Frames(0.0));
    }
    #[test]
    fn time_error_names_the_input() {
        assert!(Time::parse("12").unwrap_err().message.contains("`12`"));
    }
    #[test]
    fn color_three_digits_double_each_digit() {
        assert_eq!(
            Color::parse("#F80").unwrap(),
            Color {
                r: 255,
                g: 136,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            Color::parse("#abc").unwrap(),
            Color::parse("#AABBCC").unwrap()
        );
    }
    #[test]
    fn color_rejects_wrong_lengths_missing_hash_and_non_ascii() {
        for bad in [
            "", "#", "#12", "#1234", "#12345", "#1234567", "FFF", "#ééé", "#+12",
        ] {
            assert!(Color::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }
    #[test]
    fn size_rejects_malformed_explicit_sizes() {
        for bad in [
            "", "640x", "x360", "640x0", "-1x5", "1.5x2", "640X360", "1x2x3",
        ] {
            assert!(Size::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
