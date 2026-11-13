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

const WEIGHT_RULE: &str = "weight must be 100, 200 \u{2026} 900";

impl Font {
    /// Parses `<family> <size> [<weight>]`.
    ///
    /// The family may contain spaces and may even end in a number, so the
    /// numbers are read from the end of the string:
    ///
    /// - one trailing number is the size, and the weight is 400:
    ///   `Noto Sans Arabic 40`;
    /// - two trailing numbers are `<size> <weight>` when the last is a valid
    ///   weight (a multiple of 100 from 100 to 900): `Inter 96 700`;
    /// - two trailing numbers whose last is not a valid weight are rejected
    ///   as ambiguous, because `Baloo 2 120` could be family `Baloo`, size 2,
    ///   weight 120, or family `Baloo 2` with size 120;
    /// - three trailing numbers are a family ending in a number, then the
    ///   size and weight: `Baloo 2 120 400`.
    pub fn parse(text: &str) -> Result<Font, ShorthandError> {
        let err = |m: String| ShorthandError::new(m, EXPECTED);
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let is_number = |s: &str| s.parse::<f64>().is_ok();
        let valid_weight = |s: &str| {
            s.parse::<u16>()
                .ok()
                .filter(|w| (100..=900).contains(w) && w % 100 == 0)
        };
        let n = tokens.len();
        let trailing = tokens.iter().rev().take_while(|t| is_number(t)).count();
        let missing_family = || err(format!("`{text}` has no font family"));
        let (family_len, weight) = match trailing {
            0 => return Err(err(format!("`{text}` has no size"))),
            1 => (n - 1, 400),
            2 => match valid_weight(tokens[n - 1]) {
                Some(w) => (n - 2, w),
                None if n == 2 => return Err(missing_family()),
                None => {
                    return Err(err(format!(
                        "`{text}` is ambiguous: if the family name ends in a number, \
                         write the size and weight explicitly, e.g. `{} 400`; \
                         otherwise the {WEIGHT_RULE}",
                        tokens.join(" ")
                    )));
                }
            },
            _ => match valid_weight(tokens[n - 1]) {
                Some(w) => (n - 2, w),
                None => {
                    return Err(err(format!(
                        "`{}` is not a valid weight; the {WEIGHT_RULE}",
                        tokens[n - 1]
                    )));
                }
            },
        };
        let family_tokens = &tokens[..family_len];
        // `96 700` has no family: numbers alone must not be taken as one.
        if family_tokens.iter().all(|t| is_number(t)) {
            return Err(missing_family());
        }
        let size_text = tokens[family_len];
        let size = size_text
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or_else(|| {
                err(format!(
                    "`{size_text}` is not a valid size; it must be above zero"
                ))
            })?;
        Ok(Font {
            family: family_tokens.join(" "),
            size,
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
    #[test]
    fn font_family_ending_in_a_number_needs_explicit_size_and_weight() {
        let f = Font::parse("Baloo 2 120 400").unwrap();
        assert_eq!(
            (f.family.as_str(), f.size, f.weight),
            ("Baloo 2", 120.0, 400)
        );
    }
    #[test]
    fn font_ambiguous_trailing_numbers_are_rejected_with_the_explicit_form() {
        let e = Font::parse("Baloo 2 120").unwrap_err().to_string();
        assert!(e.contains("`Baloo 2 120` is ambiguous"), "{e}");
        assert!(e.contains("e.g. `Baloo 2 120 400`"), "{e}");
        assert!(Font::parse("Inter 96 950").is_err());
        assert!(Font::parse("Inter 96 750").is_err());
    }
    #[test]
    fn font_two_trailing_numbers_with_valid_weight_are_size_and_weight() {
        let f = Font::parse("Baloo 2 700").unwrap();
        assert_eq!((f.family.as_str(), f.size, f.weight), ("Baloo", 2.0, 700));
    }
    #[test]
    fn font_errors_name_their_cause() {
        let msg = |t: &str| Font::parse(t).unwrap_err().message;
        assert!(msg("Inter").contains("has no size"));
        assert!(msg("96 700").contains("has no font family"));
        assert!(msg("Inter 0").contains("`0` is not a valid size"));
        assert!(msg("Inter 96 700 750").contains("weight must be 100, 200"));
        assert!(msg("Inter 96 700 750").contains("`750` is not a valid weight"));
    }
}
