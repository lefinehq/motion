//! The behaviour shorthand: an entrance preset with optional tuning tokens,
//! e.g. `"fade-up 60px 12f ease-out @15f"`.
//!
//! Part of step 1 (Read): parses a layer's `in` string into a [`Behaviour`].
//! Step 2 turns it into an entrance on the compiled graph.

use crate::ShorthandError;
use crate::units::Time;

pub(crate) const EXPECTED: &str = "`<preset> [<n>px] [<n>f|<n>s] [<easing>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`; an easing is `linear`, `ease-in`, `ease-out`, `ease-in-out` or `cubic(x1,y1,x2,y2)`";

/// The expected form of a custom curve.
const CUBIC_EXPECTED: &str =
    "`cubic(x1,y1,x2,y2)` with x1 and x2 between 0 and 1, e.g. `cubic(.2,0,0,1)`";

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

/// How a behaviour's progress is eased over its duration.
///
/// Every easing is a cubic bézier from (0,0) to (1,1), the same model as CSS
/// timing functions. The names are shortcuts for common curves; `Cubic`
/// gives full control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// Starts slow, ends fast.
    EaseIn,
    /// Starts fast, ends slow.
    EaseOut,
    /// Slow at both ends.
    EaseInOut,
    /// A custom curve with control points `(x1, y1)` and `(x2, y2)`, written
    /// `cubic(x1,y1,x2,y2)`. `x1` and `x2` lie in 0..=1 so that time only
    /// moves forward; `y1` and `y2` may leave 0..=1 to overshoot or
    /// anticipate.
    Cubic(f64, f64, f64, f64),
}

impl Easing {
    /// Parses `cubic(x1,y1,x2,y2)`; `None` if `token` is not a `cubic(...)`
    /// token at all, so the caller can try other token kinds.
    fn parse_cubic(token: &str) -> Option<Result<Easing, ShorthandError>> {
        let inner = token.strip_prefix("cubic(")?;
        let err = |m: String| ShorthandError::new(m, CUBIC_EXPECTED);
        let Some(inner) = inner.strip_suffix(')') else {
            return Some(Err(err(format!("`{token}` is missing its closing `)`"))));
        };
        let values: Vec<f64> = match inner
            .split(',')
            .map(|v| v.trim().parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect::<Option<Vec<f64>>>()
        {
            Some(v) if v.len() == 4 => v,
            _ => return Some(Err(err(format!("`{token}` needs four numbers")))),
        };
        let (x1, y1, x2, y2) = (values[0], values[1], values[2], values[3]);
        // With x outside 0..=1 the curve could run backwards in time, and a
        // frame would have two values.
        if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
            return Some(Err(err(format!(
                "`{token}`: x1 and x2 must be between 0 and 1"
            ))));
        }
        Some(Ok(Easing::Cubic(x1, y1, x2, y2)))
    }
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
    pub easing: Easing,
    /// When the motion starts, measured from the start of the scene; frame 0 by default.
    pub start: Time,
}

impl Behaviour {
    /// Parses `<preset>` followed by any of `<n>px` (distance), `<n>f` or
    /// `<n>s` (duration), an easing name and `@<time>` (start).
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
        let mut easing = None;
        let mut start = None;
        for token in tokens {
            let easing_name = match token {
                "linear" => Some(Easing::Linear),
                "ease-in" => Some(Easing::EaseIn),
                "ease-out" => Some(Easing::EaseOut),
                "ease-in-out" => Some(Easing::EaseInOut),
                _ => None,
            };
            let bad = |kind: &str| err(format!("`{token}` is not a valid {kind}"));
            let easing_name = match Easing::parse_cubic(token) {
                Some(cubic) => Some(cubic?),
                None => easing_name,
            };
            if let Some(e) = easing_name {
                set_once(&mut easing, e, token, "easing", &err)?;
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
            easing: easing.map_or(Easing::EaseOut, |(v, _)| v),
            start: start.map_or(Time::Frames(0.0), |(v, _)| v),
        })
    }
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
