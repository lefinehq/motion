//! Unit tests for `src/first.rs`.

use rstest::rstest;

use super::*;
use crate::set::{FontQuery, FontSet};

#[rstest]
#[case::regular("InterVariable")]
#[case::bold("InterVariable-Bold")]
fn inters_first_baseline_is_its_d_in_whole_thousandths(#[case] name: &str) {
    let set = FontSet::builtin();
    let face = set.find(&FontQuery::PostScript(name.into())).unwrap();
    // Inter's `d` reaches 1490 of 2048 units, 0.72754 em; its ascent is 0.969.
    assert!(
        (below_top(face) - 0.728).abs() < 1e-12,
        "{}",
        below_top(face)
    );
}

#[rstest]
#[case::latin(0.7, 0.9, true)]
#[case::short_but_an_ascender(0.28, 0.9, true)]
#[case::a_symbols_d(0.09, 0.9, false)]
#[case::just_too_short(0.26, 0.9, false)]
#[case::flat(0.0, 0.9, false)]
#[case::below_the_baseline(-0.5, 0.9, false)]
#[case::broken(f64::NAN, 0.9, false)]
#[case::too_tall(5.0, 0.9, false)]
#[case::no_ascent_to_hold_it_to(0.7, f64::NAN, true)]
#[case::a_zero_ascent(0.7, 0.0, true)]
fn a_d_is_an_ascender_when_it_is_tall_enough(
    #[case] d: f64,
    #[case] ascent: f64,
    #[case] taken: bool,
) {
    assert_eq!(is_ascender(d, ascent), taken);
}
