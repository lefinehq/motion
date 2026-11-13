//! Number formatting shared by everything motion prints or writes.
//!
//! Part of step 1 (Read), though every later step uses it: one function so
//! reports, fix hints and written scenes all show numbers the same way.

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
#[path = "../tests/unit/number.rs"]
mod tests;
