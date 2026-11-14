//! Number formatting for everything motion prints or writes, so output is
//! stable and diff-friendly.

/// Formats a number with at most two decimals and no trailing zeros; values
/// that round to negative zero print as `0`.
pub fn fmt_num(value: f64) -> String {
    let text = format!("{value:.2}");
    let trimmed = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text.as_str()
    };
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
#[path = "../tests/unit/number.rs"]
mod tests;
