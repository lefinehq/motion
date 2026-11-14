//! Unit tests for `src/graph.rs`.

use super::*;
use crate::track::KeySpec;
use motion_format::ValueType;

#[test]
fn always_shows_and_keeps_the_compositions_time() {
    let t = Timing::ALWAYS;
    assert!(t.shows_at(-1e9) && t.shows_at(0.0) && t.shows_at(1e9));
    assert_eq!(t.local(42.5), 42.5);
}

#[test]
fn a_span_is_half_open() {
    let t = Timing {
        span: Some((10.0, 20.0)),
        ..Timing::ALWAYS
    };
    assert_eq!(
        (
            t.shows_at(9.99),
            t.shows_at(10.0),
            t.shows_at(19.99),
            t.shows_at(20.0)
        ),
        (false, true, true, false)
    );
}

#[test]
fn local_time_starts_at_the_offset_and_runs_at_one_over_stretch() {
    let half = Timing {
        span: None,
        offset: 10.0,
        stretch: 2.0,
    };
    assert_eq!((half.local(10.0), half.local(30.0)), (0.0, 10.0));
    let back = Timing {
        stretch: -1.0,
        ..half
    };
    assert_eq!((back.local(10.0), back.local(5.0)), (0.0, 5.0));
}

#[test]
fn a_split_track_joins_its_components() {
    let x = Track::new(
        ValueType::Number,
        vec![
            KeySpec::linear(0.0, Value::Number(0.0)),
            KeySpec::linear(10.0, Value::Number(10.0)),
        ],
        10.0,
        false,
    );
    let split = PropertyTrack::Split(vec![x, Track::constant(Value::Number(7.0))]);
    assert_eq!(
        split.sample(5.0, ValueType::Point),
        Value::Point([5.0, 7.0])
    );
    assert!(!split.is_constant());
    let still = PropertyTrack::Split(vec![
        Track::constant(Value::Number(1.0)),
        Track::constant(Value::Number(2.0)),
    ]);
    assert!(still.is_constant());
    let whole = PropertyTrack::Whole(Track::constant(Value::Point([1.0, 2.0])));
    assert_eq!(
        whole.sample(3.0, ValueType::Point),
        Value::Point([1.0, 2.0])
    );
    assert!(whole.is_constant());
}
