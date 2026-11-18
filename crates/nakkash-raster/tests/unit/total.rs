//! Unit tests for `src/total.rs`.

use rstest::rstest;

use super::*;

/// 100 a second until 0.5 s, then none.
fn burst() -> Total {
    Total::new(vec![
        TotalPiece {
            start: 0.0,
            total: 0.0,
            rate: 100.0,
            slope: 0.0,
        },
        TotalPiece {
            start: 0.5,
            total: 50.0,
            rate: 0.0,
            slope: 0.0,
        },
    ])
}

/// A held rate gives the same numbers, to the bit, as `rate·t` and `i / rate`.
#[test]
fn a_held_rate_counts_as_the_rate_alone() {
    let rate = 37.3_f32;
    let held = Total::constant(rate);
    for t in [0.0_f32, 0.1, 1.7, 9.95] {
        assert_eq!(held.at(t).to_bits(), (t * rate).to_bits());
    }
    for i in [0_u32, 1, 7, 1234] {
        let i = i as f32;
        assert_eq!(held.passes(i).map(f32::to_bits), Some((i / rate).to_bits()));
    }
}

/// After a burst the total holds, and what it made keeps its birth times.
#[rstest]
#[case::during(0.25, 25.0)]
#[case::at_the_end(0.5, 50.0)]
#[case::after(3.0, 50.0)]
fn a_burst_stops_counting(#[case] time: f32, #[case] total: f32) {
    assert!((burst().at(time) - total).abs() < 1e-4);
}

#[test]
fn a_burst_births_its_own_and_no_more() {
    let burst = burst();
    assert_eq!(burst.passes(0.0), Some(0.0));
    assert!((burst.passes(49.0).unwrap() - 0.49).abs() < 1e-6);
    // The total stops on 50, so the 51st is never born.
    assert_eq!(burst.passes(50.0), None);
}

/// A rate that rises from 0 at 100 a second each second has made
/// `50·t²` by `t`, so the 50th passes at 1 s.
#[test]
fn a_rising_rate_is_solved_exactly() {
    let ramp = Total::new(vec![TotalPiece {
        start: 0.0,
        total: 0.0,
        rate: 0.0,
        slope: 100.0,
    }]);
    assert!((ramp.at(1.0) - 50.0).abs() < 1e-5);
    assert!((ramp.passes(50.0).unwrap() - 1.0).abs() < 1e-5);
    assert_eq!(ramp.passes(0.0), Some(0.0));
}

#[test]
#[should_panic(expected = "rise")]
fn a_total_that_falls_is_refused() {
    Total::new(vec![
        TotalPiece {
            start: 0.0,
            total: 10.0,
            rate: 1.0,
            slope: 0.0,
        },
        TotalPiece {
            start: 1.0,
            total: 5.0,
            rate: 1.0,
            slope: 0.0,
        },
    ]);
}
