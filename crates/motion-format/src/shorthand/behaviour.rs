//! The behaviour shorthand: an entrance preset with optional tuning tokens,
//! e.g. `"fade-up 60px 12f ease-out @15f"`.
//!
//! Part of step 1 (Read): parses a layer's `in` string into a [`Behaviour`].
//! The preset name is a plain string here; the scene parser resolves it.
//! Step 2 turns it into an entrance on the compiled graph.

use crate::parse::{ID_RULE, is_valid_id};
use crate::units::Time;
use crate::{Curve, ShorthandError};

pub(crate) const EXPECTED: &str = "`<preset> [<n>px] [<n>f|<n>s] [<curve>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`; a curve is a name such as `ease-out`, `hold`, or `cubic(x1,y1,x2,y2)`";

/// Name of the default curve, which must be in `NAMED_CURVES`.
const DEFAULT_CURVE: &str = "ease-out";

/// A parsed behaviour: a preset plus its tuning, with defaults filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct Behaviour {
    /// The name of the preset to perform; checked against the scene's presets
    /// when the scene is parsed.
    pub preset: String,
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
        // Only the shape of the name is checked here; whether a preset of
        // that name exists depends on the scene and is checked by the parser.
        let preset = match tokens.next() {
            None => return Err(err("a behaviour needs a preset".into())),
            Some(name) if is_valid_id(name) => name.to_string(),
            Some(other) => {
                return Err(err(format!(
                    "`{other}` is not a valid preset name; {ID_RULE}"
                )));
            }
        };
        let mut distance = None;
        let mut duration = None;
        let mut curve = None;
        let mut start = None;
        for token in tokens {
            let bad = |kind: &str| err(format!("`{token}` is not a valid {kind}"));
            let has_digit = token.chars().any(|c| c.is_ascii_digit());
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
            } else if has_digit && (token.ends_with('f') || token.ends_with('s')) {
                let v = Time::parse(token).map_err(|_| bad("duration"))?;
                set_once(&mut duration, v, token, "duration", &err)?;
            } else if !has_digit {
                // No digit means no number, so it can only be a misspelt
                // curve: let `Curve::parse` list the known names.
                Curve::parse(token)?;
                return Err(err(format!("unknown token `{token}`")));
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
