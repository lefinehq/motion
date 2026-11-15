//! Unit tests for `src/shorthand/shutter.rs`.

use super::*;

#[test]
fn a_shutter_takes_one_to_three_numbers_and_defaults_the_rest() {
    let parse = |text: &str| text.parse::<Shutter>().map_err(|e| e.kind);
    assert_eq!(parse("180 -90 16"), Ok(Shutter::DEFAULT));
    assert_eq!(
        parse("360"),
        Ok(Shutter {
            angle: 360.0,
            ..Shutter::DEFAULT
        })
    );
    assert_eq!(
        parse("90 0"),
        Ok(Shutter {
            angle: 90.0,
            phase: 0.0,
            samples: 16
        })
    );
    assert!(!parse("0").unwrap().open());
    assert_eq!(parse("800"), Err(Kind::OutOfRange));
    assert_eq!(parse("180 -90 1"), Err(Kind::OutOfRange));
    assert_eq!(parse("180 -90 2.5"), Err(Kind::Invalid));
    assert_eq!(parse("wide"), Err(Kind::Invalid));
    assert_eq!(parse(""), Err(Kind::Invalid));
}

#[test]
fn samples_fall_evenly_over_the_open_shutter() {
    let moments: Vec<f64> = Shutter::DEFAULT.moments(10.0, 2).collect();
    // Open from a quarter frame before to a quarter after, halves' middles.
    assert_eq!(moments, [9.875, 10.125]);
}
