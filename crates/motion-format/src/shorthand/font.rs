//! The `font` shorthand: family, size and weight in one string.
//!
//! Part of step 1 (Read): parses a text layer's `font` string into a [`Font`].

use crate::ShorthandError;

pub(crate) const EXPECTED: &str = "`<family> <size> [<weight>]`, e.g. `Inter 96 700`";

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
#[path = "../../tests/unit/shorthand/font.rs"]
mod tests;
