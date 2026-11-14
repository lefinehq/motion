//! The fixed binary records of the format, as `binrw` structs. Each reads the
//! fields motion uses, at the offsets of the spec's table, and skips the rest.
//! [`parse`] is the one place a `binrw` error becomes a [`Diagnostic`].

use std::fmt;
use std::io::Cursor;

use binrw::BinRead;
use motion_format::{Diagnostic, Kind, Result};

use crate::rifx::Tag;

/// Reads a `T` from the start of `bytes`, big-endian unless `T` says otherwise.
/// `what` names the record in the error.
///
/// # Errors
/// A diagnostic when `bytes` is too short for the record, or a field does
/// not hold one of its allowed values.
pub(crate) fn parse<'a, T: BinRead>(
    bytes: &[u8],
    what: impl fmt::Display,
    args: T::Args<'a>,
) -> Result<T> {
    T::read_be_args(&mut Cursor::new(bytes), args).map_err(|error| {
        if error.is_eof() {
            Diagnostic::new(
                Kind::Invalid,
                format!("`{what}` holds {} bytes, too few to read", bytes.len()),
            )
            .expected(format!("a whole `{what}` record"))
        } else {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot read `{what}`: {}", error.root_cause()),
            )
        }
    })
}

/// The 8-byte header in front of every chunk body
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#chunks)).
#[derive(BinRead, Debug)]
pub(crate) struct Header {
    /// The chunk tag.
    pub tag: Tag,
    /// The body length, without the header and the pad byte.
    pub size: u32,
}

impl Header {
    /// The header's length in bytes.
    pub const SIZE: usize = 8;
}

#[cfg(test)]
#[path = "../tests/unit/record.rs"]
mod tests;
