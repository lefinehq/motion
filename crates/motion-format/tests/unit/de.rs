//! Unit tests for `src/de.rs`.

use super::*;
use serde::Deserialize;
use serde_json::json;

/// The diagnostic of reading a `T` from `value`.
fn err<T: for<'de> Deserialize<'de> + fmt::Debug>(value: Value) -> Diagnostic {
    from_value::<T>(&value).unwrap_err()
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Pair {
    a: u8,
    b: Option<u8>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Mode {
    On,
    Off,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Wrapped(Mode);

/// Fails with a message of its own.
#[derive(Debug)]
struct Refuses;

impl<'de> Deserialize<'de> for Refuses {
    fn deserialize<D: Deserializer<'de>>(_: D) -> std::result::Result<Self, D::Error> {
        Err(de::Error::custom("no"))
    }
}

#[test]
fn reads_what_serde_json_reads() {
    let pair: Pair = from_value(&json!({"a": 1, "b": null})).unwrap();
    assert_eq!(pair, Pair { a: 1, b: None });
    let pair: Pair = from_value(&json!({"a": 1, "b": 2})).unwrap();
    assert_eq!(pair.b, Some(2));
    let modes: Vec<Wrapped> = from_value(&json!(["on", "off"])).unwrap();
    assert_eq!(modes, [Wrapped(Mode::On), Wrapped(Mode::Off)]);
    let numbers: (u64, i64, f64) = from_value(&json!([1, -1, 0.5])).unwrap();
    assert_eq!(numbers, (1, -1, 0.5));
    let text: String = from_value(&json!("x")).unwrap();
    assert_eq!(text, "x");
}

#[test]
fn each_serde_error_has_its_kind() {
    let cases = [
        (err::<String>(json!(1)), Kind::WrongType),
        (err::<Mode>(json!(1)), Kind::WrongType),
        (err::<u8>(json!(-1)), Kind::Invalid),
        (err::<(u8, u8)>(json!([1])), Kind::Invalid),
        (err::<Pair>(json!({"a": 1, "c": 2})), Kind::UnknownName),
        (err::<Mode>(json!("dim")), Kind::UnknownName),
        (err::<Pair>(json!({})), Kind::Missing),
        (err::<Refuses>(json!(null)), Kind::Invalid),
    ];
    for (diagnostic, kind) in cases {
        assert_eq!(diagnostic.kind, kind, "{diagnostic}");
    }
}

#[test]
fn the_expected_form_is_kept_apart_from_the_message() {
    let e = err::<String>(json!(null));
    assert_eq!(e.message, "invalid type: null");
    assert_eq!(e.expected.as_deref(), Some("a string"));
    let e = err::<(u8, u8)>(json!([1]));
    assert_eq!(e.message, "invalid length 1");
    assert_eq!(e.expected.as_deref(), Some("a tuple of size 2"));
}

#[test]
fn unknown_names_are_listed_without_quotes() {
    assert_eq!(
        err::<Pair>(json!({"a": 1, "c": 2})).expected.as_deref(),
        Some("one of a, b")
    );
    assert_eq!(
        err::<Mode>(json!("dim")).expected.as_deref(),
        Some("one of on, off")
    );
}

#[test]
fn a_failure_sits_at_its_path() {
    assert_eq!(err::<String>(json!(1)).path, None);
    assert_eq!(
        err::<Vec<Pair>>(json!([{"a": 1}, {"a": "x"}]))
            .path
            .as_deref(),
        Some("[1].a")
    );
    assert_eq!(
        err::<Pair>(json!({"a": 1, "c": 2})).path.as_deref(),
        Some("c")
    );
}

#[test]
fn a_float_is_named_in_its_shortest_form() {
    assert_eq!(
        err::<String>(json!(1e300)).message,
        "invalid type: floating point `1e300`"
    );
    assert_eq!(
        err::<String>(json!(1.5)).message,
        "invalid type: floating point `1.5`"
    );
}
