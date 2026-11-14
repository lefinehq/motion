//! Unit tests for `src/value.rs`.

use super::*;
use crate::Kind;
use proptest::prelude::*;

#[test]
fn each_type_knows_its_component_count() {
    assert_eq!(ValueType::Number.dims(), Some(1));
    assert_eq!(ValueType::Point.dims(), Some(2));
    assert_eq!(ValueType::Color.dims(), Some(4));
    assert_eq!(ValueType::Path.dims(), None);
}

#[test]
fn components_round_trip_through_every_type() {
    for value in [
        Value::Number(2.5),
        Value::Point([3.0, -4.0]),
        Value::Color([1.0, 0.5, 0.0, 1.0]),
    ] {
        let ty = value.value_type();
        assert_eq!(Some(value.components().len()), ty.dims());
        assert_eq!(Value::from_components(ty, value.components()), value);
    }
}

#[test]
#[should_panic(expected = "components")]
fn from_components_rejects_a_wrong_count() {
    Value::from_components(ValueType::Point, &[1.0]);
}

#[test]
fn splat_sets_every_component() {
    assert_eq!(
        Value::splat(ValueType::Point, 0.6),
        Value::Point([0.6, 0.6])
    );
    assert_eq!(Value::splat(ValueType::Number, 3.0), Value::Number(3.0));
}

#[test]
fn accessors_answer_only_for_their_type() {
    assert_eq!(Value::Number(1.0).as_number(), Some(1.0));
    assert_eq!(Value::Point([1.0, 2.0]).as_number(), None);
    assert_eq!(Value::Point([1.0, 2.0]).as_point(), Some([1.0, 2.0]));
    assert_eq!(Value::Number(1.0).as_point(), None);
}

#[test]
fn parse_reads_each_type_from_its_string_form() {
    assert_eq!(
        Value::parse("12", ValueType::Number, false).unwrap(),
        Value::Number(12.0)
    );
    assert_eq!(
        Value::parse("540,960", ValueType::Point, false).unwrap(),
        Value::Point([540.0, 960.0])
    );
    assert_eq!(
        Value::parse("#FF0000", ValueType::Color, false).unwrap(),
        Value::Color([1.0, 0.0, 0.0, 1.0])
    );
    assert_eq!(
        Value::parse("1,0.5,0", ValueType::Color, false).unwrap(),
        Value::Color([1.0, 0.5, 0.0, 1.0])
    );
}

#[test]
fn a_lone_number_sets_a_point_only_when_it_broadcasts() {
    assert_eq!(
        Value::parse("0.8", ValueType::Point, true).unwrap(),
        Value::Point([0.8, 0.8])
    );
    let error = Value::parse("0.8", ValueType::Point, false).unwrap_err();
    assert_eq!(error.kind, Kind::Syntax);
    assert!(error.message.contains("not a point"), "{error}");
    assert_eq!(error.expected.as_deref(), Some(ValueType::Point.form()));
}

#[test]
fn parse_rejects_text_that_is_not_the_type() {
    for (text, ty) in [
        ("abc", ValueType::Number),
        ("1,2", ValueType::Number),
        ("1,2,3", ValueType::Point),
        ("#GGGGGG", ValueType::Color),
        ("", ValueType::Number),
        ("1,", ValueType::Point),
    ] {
        assert!(Value::parse(text, ty, false).is_err(), "{text:?} as {ty:?}");
    }
}

#[test]
fn from_numbers_fits_the_count_to_the_type() {
    assert_eq!(
        Value::from_numbers(&[1.0, 2.0], ValueType::Point, false).unwrap(),
        Value::Point([1.0, 2.0])
    );
    // A colour may leave alpha out.
    assert_eq!(
        Value::from_numbers(&[1.0, 0.5, 0.25], ValueType::Color, false).unwrap(),
        Value::Color([1.0, 0.5, 0.25, 1.0])
    );
    let error = Value::from_numbers(&[1.0, 2.0, 3.0], ValueType::Point, false).unwrap_err();
    assert_eq!(error.kind, Kind::WrongType);
    assert_eq!(error.message, "3 numbers do not make a point");
}

#[test]
fn an_eight_bit_colour_converts_channel_by_channel() {
    let red: Color = "#FF000080".parse().unwrap();
    assert_eq!(
        Value::from(red),
        Value::Color([1.0, 0.0, 0.0, 128.0 / 255.0])
    );
}

#[test]
fn display_writes_the_string_form() {
    assert_eq!(Value::Number(0.5).to_string(), "0.5");
    assert_eq!(Value::Point([540.0, -12.25]).to_string(), "540,-12.25");
    assert_eq!(
        Value::Color([1.0, 224.0 / 255.0, 0.0, 1.0]).to_string(),
        "#FFE000"
    );
    assert_eq!(
        Value::Color([1.0, 0.0, 0.0, 128.0 / 255.0]).to_string(),
        "#FF000080"
    );
    // A channel between 8-bit steps keeps its exact value.
    assert_eq!(
        Value::Color([1.0, 0.88, 0.0, 1.0]).to_string(),
        "1,0.88,0,1"
    );
}

proptest! {
    #[test]
    fn parse_never_panics(text in ".{0,24}", broadcast: bool) {
        for ty in [ValueType::Number, ValueType::Point, ValueType::Color] {
            let _ = Value::parse(&text, ty, broadcast);
        }
    }

    #[test]
    fn a_point_round_trips_through_its_string_form(x in -1e4f64..1e4, y in -1e4f64..1e4) {
        let value = Value::Point([(x * 100.0).round() / 100.0, (y * 100.0).round() / 100.0]);
        let back = Value::parse(&value.to_string(), ValueType::Point, false).unwrap();
        prop_assert_eq!(back, value);
    }
}

fn square() -> Value {
    Value::parse("M0,0 L10,0 L10,10 Z", ValueType::Path, false).unwrap()
}

#[test]
fn a_path_reads_and_writes_as_path_data() {
    let value = square();
    assert_eq!(value.value_type(), ValueType::Path);
    assert_eq!(value.as_path().unwrap().len(), 3);
    assert_eq!(value.components().len(), 18);
    let written = value.to_string();
    assert_eq!(
        Value::parse(&written, ValueType::Path, false).unwrap(),
        value
    );
}

#[test]
fn with_components_keeps_the_shape() {
    let value = square();
    let moved = value.with_components(&[1.0; 18]);
    assert!(moved.same_shape(&value));
    assert!(moved.as_path().unwrap().closed());
    assert_eq!(
        Value::Point([1.0, 2.0]).with_components(&[3.0, 4.0]),
        Value::Point([3.0, 4.0])
    );
}

#[test]
fn same_shape_compares_type_and_path_shape() {
    let open = Value::parse("M0,0 L10,0 L10,10", ValueType::Path, false).unwrap();
    assert!(!square().same_shape(&open));
    assert!(Value::Number(1.0).same_shape(&Value::Number(2.0)));
    assert!(!Value::Number(1.0).same_shape(&Value::Point([1.0, 1.0])));
}

#[test]
fn a_path_is_not_written_as_numbers() {
    let error = Value::from_numbers(&[1.0, 2.0], ValueType::Path, false).unwrap_err();
    assert_eq!(error.kind, Kind::WrongType);
    assert!(error.message.contains("path data"));
}

#[test]
#[should_panic(expected = "fixed-size")]
fn from_components_needs_a_fixed_size_type() {
    Value::from_components(ValueType::Path, &[]);
}
