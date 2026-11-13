//! Number formatting shared by everything motion prints or writes.

/// Formats a number with at most two decimals and no trailing zeros.
///
/// Everything motion prints or writes goes through this function so output is
/// stable and diff-friendly. Negative zero (and values that round to it)
/// print as `0`.
pub fn fmt_num(value: f64) -> String {
    let text = format!("{value:.2}");
    let trimmed = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text.as_str()
    };
    // "-0" comes from -0.0 or from small negatives that round to zero.
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_num_rounds_and_trims() {
        assert_eq!(fmt_num(960.0), "960");
        assert_eq!(fmt_num(0.5), "0.5");
        assert_eq!(fmt_num(1.234), "1.23");
        assert_eq!(fmt_num(-0.001), "0");
        assert_eq!(fmt_num(580.8), "580.8");
    }

    #[test]
    fn fmt_num_handles_negative_zero_and_negatives() {
        assert_eq!(fmt_num(-0.0), "0");
        assert_eq!(fmt_num(-2.5), "-2.5");
        assert_eq!(fmt_num(-100.0), "-100");
    }

    #[test]
    fn fmt_num_keeps_trailing_zeros_of_integers() {
        // Trimming must stop at the decimal point, never eat integer digits.
        assert_eq!(fmt_num(100.0), "100");
        assert_eq!(fmt_num(10.0), "10");
        assert_eq!(fmt_num(0.0), "0");
    }
}
