//! The `shutter` shorthand, `"180 -90 16"`: how long a composition's shutter
//! stays open for motion blur, where it opens, and how many samples a frame
//! takes ([D57]).
//!
//! [D57]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d57-motion-blur-samples-a-layers-motion-over-the-shutter

use std::str::FromStr;

use crate::doc::Form;
use crate::{Diagnostic, Kind, Result};

pub(crate) const EXPECTED: &str = "`<angle 0-720> [<phase -360-360> [<samples 2-64>]]`, e.g. `180 -90 16`, or `0` for no motion blur";

/// A composition's shutter: what its layers with `mblur` blur over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shutter {
    /// How long it stays open, in degrees of a frame: 360 is a whole frame.
    pub angle: f64,
    /// Where it opens, in degrees of a frame from the frame's time.
    pub phase: f64,
    /// How many moments a frame samples, at least; a fast layer takes up
    /// to [`Shutter::MAX_GROWTH`] times as many.
    pub samples: u32,
}

impl Shutter {
    /// The shutter of a composition that does not set one.
    pub const DEFAULT: Shutter = Shutter {
        angle: 180.0,
        phase: -90.0,
        samples: 16,
    };

    /// How many times `samples` a layer that moves fast may take.
    pub const MAX_GROWTH: u32 = 8;

    /// Whether it is open at all.
    pub fn open(self) -> bool {
        self.angle > 0.0
    }

    /// When the `n` samples of the frame at `frame` fall, in frames: evenly
    /// over the open shutter, each in the middle of its share.
    pub fn moments(self, frame: f64, n: u32) -> impl Iterator<Item = f64> {
        let n = n.max(1);
        (0..n).map(move |k| {
            frame + (self.phase + self.angle * (f64::from(k) + 0.5) / f64::from(n)) / 360.0
        })
    }
}

impl Default for Shutter {
    fn default() -> Shutter {
        Shutter::DEFAULT
    }
}

impl FromStr for Shutter {
    type Err = Diagnostic;

    /// Parses `<angle> [<phase> [<samples>]]`.
    fn from_str(text: &str) -> Result<Shutter> {
        let invalid = |what: String| Diagnostic::new(Kind::Invalid, what).expected(EXPECTED);
        let numbers = text
            .split_whitespace()
            .map(|word| {
                word.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| invalid(format!("`{word}` is not a number")))
            })
            .collect::<Result<Vec<f64>>>()?;
        let within = |n: f64, (lo, hi): (f64, f64), what: &str| match (lo..=hi).contains(&n) {
            true => Ok(n),
            false => Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("a shutter {what} of {n}, outside {lo} to {hi}"),
            )
            .expected(EXPECTED)),
        };
        let shutter = match numbers[..] {
            [angle] => Shutter {
                angle,
                ..Shutter::DEFAULT
            },
            [angle, phase] => Shutter {
                angle,
                phase,
                ..Shutter::DEFAULT
            },
            [angle, phase, samples] if samples.fract() == 0.0 => Shutter {
                angle,
                phase,
                samples: within(samples, (2.0, 64.0), "sample count")? as u32,
            },
            [_, _, samples] => {
                return Err(invalid(format!("{samples} samples is not a whole number")));
            }
            _ => return Err(invalid(format!("`{text}` is not one to three numbers"))),
        };
        within(shutter.angle, (0.0, 720.0), "angle")?;
        within(shutter.phase, (-360.0, 360.0), "phase")?;
        Ok(shutter)
    }
}

impl Form for Shutter {
    const FORM: &str = r#"a shutter string, e.g. "180 -90 16""#;
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/shutter.rs"]
mod tests;
