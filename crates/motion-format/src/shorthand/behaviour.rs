//! The behaviour shorthand: an entrance preset with optional tuning tokens,
//! e.g. `"fade-up 60px 12f ease-out @15f"`.
//!
//! Part of step 1 (Read): parses a layer's `in` string into a [`Behaviour`].
//! Step 2 turns it into an entrance on the compiled graph.

use crate::units::Time;
use crate::{Curve, ShorthandError};

pub(crate) const EXPECTED: &str = "`<preset> [<n>px] [<n>f|<n>s] [<curve>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`; a curve is a name such as `ease-out`, `hold`, or `cubic(x1,y1,x2,y2)`";

/// Name of the default curve, which must be in `NAMED_CURVES`.
const DEFAULT_CURVE: &str = "ease-out";

/// The motion an entrance performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Fade in without moving.
    Fade,
    /// Fade in while rising from below.
    FadeUp,
    /// Fade in while sinking from above.
    FadeDown,
    /// Fade in while moving in from the right, heading left.
    FadeLeft,
    /// Fade in while moving in from the left, heading right.
    FadeRight,
}

/// A parsed behaviour: a preset plus its tuning, with defaults filled in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Behaviour {
    /// The motion to perform.
    pub preset: Preset,
    /// Travel distance in pixels; 40 by default.
    pub distance: f64,
    /// How long the motion lasts; 12 frames by default.
    pub duration: Time,
    /// Easing curve; `ease-out` by default.
    pub curve: Curve,
    /// When the motion starts, measured from the start of the scene; frame 0 by default.
    pub start: Time,
}

impl Behaviour {
    /// Parses `<preset>` followed by any of `<n>px` (distance), `<n>f` or
    /// `<n>s` (duration), a curve and `@<time>` (start).
    ///
    /// Tokens after the preset are classified by their shape, never by their
    /// position. An agent can write them in any order and still get the same
    /// result, and a given string has exactly one meaning, so there is no
    /// "which number is the distance?" guess to get wrong. Each kind may
    /// appear once.
    pub fn parse(text: &str) -> Result<Behaviour, ShorthandError> {
        let err = |m: String| ShorthandError::new(m, EXPECTED);
        let mut tokens = text.split_whitespace();
        let preset = match tokens.next() {
            None => return Err(err("a behaviour needs a preset".into())),
            Some("fade") => Preset::Fade,
            Some("fade-up") => Preset::FadeUp,
            Some("fade-down") => Preset::FadeDown,
            Some("fade-left") => Preset::FadeLeft,
            Some("fade-right") => Preset::FadeRight,
            Some(other) => return Err(err(format!("unknown preset `{other}`"))),
        };
        let mut distance = None;
        let mut duration = None;
        let mut curve = None;
        let mut start = None;
        for token in tokens {
            let bad = |kind: &str| err(format!("`{token}` is not a valid {kind}"));
            if Curve::is_curve_token(token) {
                set_once(&mut curve, Curve::parse(token)?, token, "curve", &err)?;
            } else if let Some(n) = token.strip_suffix("px") {
                let v: f64 = n
                    .parse()
                    .ok()
                    .filter(|v: &f64| v.is_finite())
                    .ok_or_else(|| bad("distance"))?;
                set_once(&mut distance, v, token, "distance", &err)?;
            } else if let Some(t) = token.strip_prefix('@') {
                let v = Time::parse(t).map_err(|_| bad("start time"))?;
                set_once(&mut start, v, token, "start", &err)?;
            } else if token.ends_with('f') || token.ends_with('s') {
                let v = Time::parse(token).map_err(|_| bad("duration"))?;
                set_once(&mut duration, v, token, "duration", &err)?;
            } else {
                return Err(err(format!("unknown token `{token}`")));
            }
        }
        Ok(Behaviour {
            preset,
            distance: distance.map_or(40.0, |(v, _)| v),
            duration: duration.map_or(Time::Frames(12.0), |(v, _)| v),
            curve: curve.map_or_else(default_curve, |(v, _)| v),
            start: start.map_or(Time::Frames(0.0), |(v, _)| v),
        })
    }
}

/// The curve used when none is given, looked up by name in the curve table.
fn default_curve() -> Curve {
    Curve::named(DEFAULT_CURVE).expect("the default curve is in NAMED_CURVES")
}

/// Stores `value` (with the token it came from) in `slot`, failing with both
/// tokens if that kind of token was already seen.
fn set_once<'a, T>(
    slot: &mut Option<(T, &'a str)>,
    value: T,
    token: &'a str,
    what: &str,
    err: &impl Fn(String) -> ShorthandError,
) -> Result<(), ShorthandError> {
    if let Some((_, first)) = slot.replace((value, token)) {
        return Err(err(format!("{what} given twice: `{first}` and `{token}`")));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/shorthand/behaviour.rs"]
mod tests;
