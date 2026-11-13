//! The behaviour shorthand: an entrance preset with optional tuning tokens,
//! e.g. `"fade-up 60px 12f ease-out @15f"`.

use crate::ShorthandError;
use crate::units::Time;

const EXPECTED: &str =
    "`<preset> [<n>px] [<n>f|<n>s] [<easing>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`";

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// Starts slow, ends fast.
    EaseIn,
    /// Starts fast, ends slow.
    EaseOut,
    /// Slow at both ends.
    EaseInOut,
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
    /// When the motion starts, relative to the layer; frame 0 by default.
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
            let bad_value = || err(format!("`{token}` is not a valid value"));
            if let Some(e) = easing_name {
                set_once(&mut easing, e, "easing", &err)?;
            } else if let Some(n) = token.strip_suffix("px") {
                let v: f64 = n
                    .parse()
                    .ok()
                    .filter(|v: &f64| v.is_finite())
                    .ok_or_else(bad_value)?;
                set_once(&mut distance, v, "distance", &err)?;
            } else if let Some(t) = token.strip_prefix('@') {
                let v = Time::parse(t).map_err(|_| bad_value())?;
                set_once(&mut start, v, "start", &err)?;
            } else if token.ends_with('f') || token.ends_with('s') {
                let v = Time::parse(token).map_err(|_| bad_value())?;
                set_once(&mut duration, v, "duration", &err)?;
            } else {
                return Err(err(format!("unknown token `{token}`")));
            }
        }
        Ok(Behaviour {
            preset,
            distance: distance.unwrap_or(40.0),
            duration: duration.unwrap_or(Time::Frames(12.0)),
            easing: easing.unwrap_or(Easing::EaseOut),
            start: start.unwrap_or(Time::Frames(0.0)),
        })
    }
}

/// Stores `value` in `slot`, failing if that kind of token was already seen.
fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    what: &str,
    err: &impl Fn(String) -> ShorthandError,
) -> Result<(), ShorthandError> {
    if slot.replace(value).is_some() {
        return Err(err(format!("{what} given twice")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Time;
    #[test]
    fn behaviour_uses_defaults() {
        let b = Behaviour::parse("fade-up").unwrap();
        assert_eq!(
            b,
            Behaviour {
                preset: Preset::FadeUp,
                distance: 40.0,
                duration: Time::Frames(12.0),
                easing: Easing::EaseOut,
                start: Time::Frames(0.0)
            }
        );
    }
    #[test]
    fn behaviour_reads_every_token_in_any_order() {
        let b = Behaviour::parse("fade-up 60px 12f ease-out @15f").unwrap();
        assert_eq!(
            (b.distance, b.duration, b.start),
            (60.0, Time::Frames(12.0), Time::Frames(15.0))
        );
        assert_eq!(
            Behaviour::parse("fade @0.5s linear 0.4s").unwrap().duration,
            Time::Seconds(0.4)
        );
    }
    #[test]
    fn behaviour_rejects_unknown_and_repeated_tokens() {
        let e = Behaviour::parse("fade-upp").unwrap_err().to_string();
        assert!(e.contains("unknown preset `fade-upp`"));
        assert!(e.contains("`<preset> [<n>px] [<n>f|<n>s] [<easing>] [@<time>]`, e.g. `fade-up 40px 12f ease-out @0f`"));
        assert!(
            Behaviour::parse("fade 12f 10f")
                .unwrap_err()
                .to_string()
                .contains("duration given twice")
        );
        assert!(
            Behaviour::parse("fade bounce")
                .unwrap_err()
                .to_string()
                .contains("unknown token `bounce`")
        );
    }
    #[test]
    fn behaviour_parses_every_preset_and_easing() {
        let presets = [
            ("fade", Preset::Fade),
            ("fade-up", Preset::FadeUp),
            ("fade-down", Preset::FadeDown),
            ("fade-left", Preset::FadeLeft),
            ("fade-right", Preset::FadeRight),
        ];
        for (name, preset) in presets {
            assert_eq!(Behaviour::parse(name).unwrap().preset, preset);
        }
        let easings = [
            ("linear", Easing::Linear),
            ("ease-in", Easing::EaseIn),
            ("ease-out", Easing::EaseOut),
            ("ease-in-out", Easing::EaseInOut),
        ];
        for (name, easing) in easings {
            assert_eq!(
                Behaviour::parse(&format!("fade {name}")).unwrap().easing,
                easing
            );
        }
    }
    #[test]
    fn behaviour_order_does_not_matter() {
        let a = Behaviour::parse("fade-down 20px 0.3s linear @2f").unwrap();
        let b = Behaviour::parse("fade-down @2f linear 0.3s 20px").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.distance, 20.0);
    }
    #[test]
    fn behaviour_rejects_each_repeated_kind() {
        for (text, what) in [
            ("fade 1px 2px", "distance"),
            ("fade linear ease-in", "easing"),
            ("fade @1f @2f", "start"),
        ] {
            let e = Behaviour::parse(text).unwrap_err().to_string();
            assert!(e.contains(&format!("{what} given twice")), "{text}: {e}");
        }
    }
    #[test]
    fn behaviour_rejects_empty_and_malformed_values() {
        for bad in [
            "",
            "   ",
            "fade @",
            "fade @12",
            "fade xpx",
            "fade -3f",
            "fade 1.2.3s",
            "fade nanpx",
            "fade-up fade-up",
        ] {
            assert!(Behaviour::parse(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(
            Behaviour::parse("")
                .unwrap_err()
                .to_string()
                .contains("needs a preset")
        );
    }
    #[test]
    fn behaviour_allows_negative_distance() {
        assert_eq!(Behaviour::parse("fade-up -10px").unwrap().distance, -10.0);
    }
}
