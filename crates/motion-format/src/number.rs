//! Number formatting for everything motion prints or writes, so output is
//! stable and diff-friendly.

/// Formats a number with at most two decimals and no trailing zeros; values
/// that round to negative zero print as `0`.
pub fn fmt_num(value: f64) -> String {
    let text = format!("{value:.2}");
    // A finite number always gets a `.`, so trimming never reaches its integer digits.
    match text.trim_end_matches('0').trim_end_matches('.') {
        "-0" => "0".to_owned(),
        trimmed => trimmed.to_owned(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/number.rs"]
mod tests;
