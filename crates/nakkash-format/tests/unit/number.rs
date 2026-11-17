//! Unit tests for `src/number.rs`.

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
