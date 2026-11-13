//! The `font` shorthand: family, size and weight in one string.

use crate::ShorthandError;

const EXPECTED: &str = "`<family> <size> [<weight>]`, e.g. `Inter 96 700`";

/// A text font: family, size in pixels and weight.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// Family name; may contain spaces, e.g. `Noto Sans Arabic`.
    pub family: String,
    /// Size in pixels, above zero.
    pub size: f64,
    /// Weight from 100 to 900; 400 when not written.
    pub weight: u16,
}

impl Font {
    /// Parses `<family> <size> [<weight>]`. The family may contain spaces, so
    /// the numbers are read from the end: a trailing number pair is
    /// `<size> <weight>`, a single trailing number is the size.
    pub fn parse(text: &str) -> Result<Font, ShorthandError> {
        let bad = || ShorthandError::new(format!("`{text}` is not a valid font"), EXPECTED);
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let size_of = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0);
        let is_number = |s: &str| s.parse::<f64>().is_ok();
        let n = tokens.len();
        // A weight is only present when the two tokens before the family are
        // both numbers; otherwise the last token is the size.
        let (family_len, size, weight) =
            if n >= 3 && is_number(tokens[n - 2]) && is_number(tokens[n - 1]) {
                let weight: u16 = tokens[n - 1].parse().map_err(|_| bad())?;
                if !(100..=900).contains(&weight) {
                    return Err(bad());
                }
                (n - 2, tokens[n - 2], weight)
            } else if n >= 2 {
                (n - 1, tokens[n - 1], 400)
            } else {
                return Err(bad());
            };
        let family = tokens[..family_len].join(" ");
        // `96 700` has no family: its first number must not be taken as one.
        if is_number(&family) {
            return Err(bad());
        }
        Ok(Font {
            family,
            size: size_of(size).ok_or_else(bad)?,
            weight,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn font_parses_family_with_spaces_size_and_weight() {
        assert_eq!(
            Font::parse("Inter 96 700").unwrap(),
            Font {
                family: "Inter".into(),
                size: 96.0,
                weight: 700
            }
        );
        assert_eq!(
            Font::parse("Noto Sans Arabic 40").unwrap(),
            Font {
                family: "Noto Sans Arabic".into(),
                size: 40.0,
                weight: 400
            }
        );
        assert!(
            Font::parse("Inter")
                .unwrap_err()
                .to_string()
                .contains("`<family> <size> [<weight>]`, e.g. `Inter 96 700`")
        );
        assert!(Font::parse("Inter 96 950").is_err());
    }
    #[test]
    fn font_size_without_weight_defaults_to_400() {
        let f = Font::parse("Inter 96").unwrap();
        assert_eq!((f.family.as_str(), f.size, f.weight), ("Inter", 96.0, 400));
    }
    #[test]
    fn font_weight_bounds_are_inclusive() {
        assert_eq!(Font::parse("A 10 100").unwrap().weight, 100);
        assert_eq!(Font::parse("A 10 900").unwrap().weight, 900);
        assert!(Font::parse("A 10 99").is_err());
    }
    #[test]
    fn font_rejects_missing_family_and_bad_sizes() {
        for bad in [
            "",
            "96",
            "96 700",
            "Inter 0",
            "Inter -4",
            "Inter big",
            "Inter 96 700.5",
        ] {
            assert!(Font::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
