//! A serde deserializer over a JSON [`Value`] whose errors are [`Diagnostic`]s.
//!
//! The kind of each error comes from the `serde::de::Error` method serde calls,
//! so no error text is ever read back.

use std::fmt;

use serde::de::value::{MapDeserializer, SeqDeserializer, StrDeserializer};
use serde::de::{self, Deserialize, Deserializer, Expected, IntoDeserializer, Unexpected, Visitor};
use serde::forward_to_deserialize_any;
use serde_json::{Number, Value};

use crate::{Diagnostic, Kind};

/// What every deserializer method returns.
type Result<T> = std::result::Result<T, Error>;

/// Reads a `T` from `value`, with the path of a failure in the diagnostic.
pub(crate) fn from_value<'de, T: Deserialize<'de>>(value: &'de Value) -> crate::Result<T> {
    serde_path_to_error::deserialize(ValueDeserializer(value)).map_err(|error| {
        let at_root = error.path().iter().len() == 0;
        let path = error.path().to_string();
        let Error(diagnostic) = error.into_inner();
        if at_root {
            diagnostic
        } else {
            diagnostic.at(path)
        }
    })
}

/// A deserialization failure, already in the shape of a [`Diagnostic`].
///
/// A newtype so that `Diagnostic` itself does not gain `serde::de::Error`'s
/// constructors, which would bypass its builder.
#[derive(Debug)]
pub(crate) struct Error(Diagnostic);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Error(Diagnostic::new(Kind::Invalid, message.to_string()))
    }

    fn invalid_type(found: Unexpected<'_>, expected: &dyn Expected) -> Self {
        Error(
            Diagnostic::new(Kind::WrongType, format!("invalid type: {}", Found(found)))
                .expected(expected.to_string()),
        )
    }

    fn invalid_value(found: Unexpected<'_>, expected: &dyn Expected) -> Self {
        Error(
            Diagnostic::new(Kind::Invalid, format!("invalid value: {}", Found(found)))
                .expected(expected.to_string()),
        )
    }

    fn invalid_length(len: usize, expected: &dyn Expected) -> Self {
        Error(
            Diagnostic::new(Kind::Invalid, format!("invalid length {len}"))
                .expected(expected.to_string()),
        )
    }

    fn unknown_variant(variant: &str, expected: &'static [&'static str]) -> Self {
        Error(Diagnostic::unknown("variant", variant, expected))
    }

    fn unknown_field(field: &str, expected: &'static [&'static str]) -> Self {
        Error(Diagnostic::unknown("field", field, expected))
    }

    fn missing_field(field: &'static str) -> Self {
        Error(Diagnostic::new(
            Kind::Missing,
            format!("missing field `{field}`"),
        ))
    }
}

/// A found value as JSON names it: serde's "unit value" is `null` here.
struct Found<'a>(Unexpected<'a>);

impl fmt::Display for Found<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Unexpected::Unit => f.write_str("null"),
            // Shortest round-trip form, so `1e300` is not printed digit by digit.
            Unexpected::Float(v) => write!(f, "floating point `{v:?}`"),
            other => other.fmt(f),
        }
    }
}

/// Hands one JSON value to a visitor.
#[derive(Clone, Copy)]
struct ValueDeserializer<'de>(&'de Value);

impl<'de> Deserializer<'de> for ValueDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.0 {
            Value::Null => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(*b),
            Value::Number(n) => visit_number(n, visitor),
            Value::String(s) => visitor.visit_borrowed_str(s),
            Value::Array(items) => {
                SeqDeserializer::new(items.iter().map(ValueDeserializer)).deserialize_any(visitor)
            }
            Value::Object(entries) => MapDeserializer::new(
                entries
                    .iter()
                    .map(|(key, value)| (key.as_str(), ValueDeserializer(value))),
            )
            .deserialize_any(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match self.0 {
            Value::String(s) => visitor.visit_enum(StrDeserializer::new(s)),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

impl<'de> IntoDeserializer<'de, Error> for ValueDeserializer<'de> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

/// Visits a JSON number as the integer or float it was written as.
fn visit_number<'de, V: Visitor<'de>>(n: &Number, visitor: V) -> Result<V::Value> {
    match (n.as_u64(), n.as_i64(), n.as_f64()) {
        (Some(u), _, _) => visitor.visit_u64(u),
        (None, Some(i), _) => visitor.visit_i64(i),
        (None, None, Some(f)) => visitor.visit_f64(f),
        // Only an `arbitrary_precision` number fits none of them.
        (None, None, None) => Err(de::Error::invalid_type(
            Unexpected::Other("number"),
            &visitor,
        )),
    }
}

#[cfg(test)]
#[path = "../tests/unit/de.rs"]
mod tests;
