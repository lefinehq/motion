//! Reads typed records out of a [`Cos`] tree with serde, as `serde_json`
//! does out of its `Value`: a record names its keys with
//! `#[serde(rename = "53")]`, so the table of keys is the record itself.
//!
//! Dictionaries read as maps and structs, arrays as sequences, names as
//! strings (and `/nil` as a missing option), integers and reals as numbers,
//! and strings as their text. [`read`] is the one place a serde error becomes
//! a [`Diagnostic`], at the key path where it happened.

use std::fmt;

use motion_format::{Diagnostic, Kind, Result};
use serde::de::value::{MapDeserializer, SeqDeserializer};
use serde::de::{self, Deserialize, Deserializer, IntoDeserializer, Visitor};
use serde::forward_to_deserialize_any;

use super::Cos;

/// The name that stands for no value.
const NIL: &str = "nil";

/// Reads a `T` out of `cos`.
///
/// # Errors
/// An invalid-value [`Diagnostic`] at the key path that does not have the
/// shape `T` expects, written as the spec writes paths (`/1/1/0`).
pub(crate) fn read<'de, T: Deserialize<'de>>(cos: &'de Cos) -> Result<T> {
    serde_path_to_error::deserialize(cos).map_err(|error| {
        let path = key_path(error.path());
        Diagnostic::new(Kind::Invalid, format!("text document: {}", error.inner()))
            .at(path)
            .expected("a text document as the format notes describe it")
    })
}

/// A serde path as the spec writes it: `/1/1/0`, `/` for the root.
fn key_path(path: &serde_path_to_error::Path) -> String {
    let path: String = path
        .iter()
        .map(|segment| format!("/{}", segment.to_string().trim_start_matches('.')))
        .collect();
    match path.as_str() {
        "" | "/" => "/".into(),
        _ => path.replace("/[", "/").replace(']', ""),
    }
}

/// Why a [`Cos`] value does not fit the record read from it.
#[derive(Debug)]
pub(crate) struct Mismatch(String);

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Mismatch {}

impl de::Error for Mismatch {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Mismatch(message.to_string())
    }
}

impl<'de> Deserializer<'de> for &'de Cos {
    type Error = Mismatch;

    fn deserialize_any<V: Visitor<'de>>(
        self,
        visitor: V,
    ) -> std::result::Result<V::Value, Mismatch> {
        match self {
            Cos::Dict(entries) => {
                let entries = entries.iter().map(|(key, value)| (key.as_str(), value));
                let mut map = MapDeserializer::new(entries);
                let value = visitor.visit_map(&mut map)?;
                map.end()?;
                Ok(value)
            }
            Cos::Array(items) => {
                let mut seq = SeqDeserializer::new(items.iter());
                let value = visitor.visit_seq(&mut seq)?;
                seq.end()?;
                Ok(value)
            }
            Cos::Name(name) => visitor.visit_borrowed_str(name),
            Cos::Integer(n) => visitor.visit_i64(*n),
            Cos::Real(x) => visitor.visit_f64(*x),
            Cos::Bool(b) => visitor.visit_bool(*b),
            Cos::String(s) => visitor.visit_string(s.text()),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(
        self,
        visitor: V,
    ) -> std::result::Result<V::Value, Mismatch> {
        match self {
            Cos::Name(name) if name == NIL => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de> IntoDeserializer<'de, Mismatch> for &'de Cos {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cos/de.rs"]
mod tests;
