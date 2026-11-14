//! The chunk tree of a RIFX file: the `RIFX` header and its `Egg!` form,
//! chunks, and `LIST`s, with bodies padded to even sizes
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#file-structure)).
//!
//! Generic: the only tag it knows is `LIST`. Lists are read lazily, one level
//! at a time, so a list that holds raw bytes instead of chunks (a text
//! document's `LIST:btdk`) is never misread unless `decode` asks for it.

use std::fmt;

use binrw::BinRead;
use motion_format::{Diagnostic, Kind, Result};

use crate::record::{self, Header};

/// A four-character code: a chunk tag or a list type, such as `cdta` or `Item`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BinRead)]
pub(crate) struct Tag(pub [u8; 4]);

impl Tag {
    /// The tag of a chunk whose body starts with a list type.
    pub const LIST: Tag = Tag(*b"LIST");
    /// The tag of the file's root chunk.
    const RIFX: Tag = Tag(*b"RIFX");
    /// The root chunk's list type.
    const FORM: Tag = Tag(*b"Egg!");
}

/// The code as text, with trailing spaces dropped (`Pin ` shows as `Pin`).
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.escape_ascii().to_string().trim_end())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{self}`")
    }
}

/// What a chunk is: its tag and, for a `LIST`, its list type. The spec names
/// lists by their type (`LIST:Item`), and so does `decode`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct Name {
    /// The chunk tag.
    pub tag: Tag,
    /// The list type, for a `LIST`.
    pub list: Option<Tag>,
}

impl Name {
    /// The name of a data chunk with `tag`.
    pub const fn chunk(tag: &[u8; 4]) -> Self {
        Self {
            tag: Tag(*tag),
            list: None,
        }
    }

    /// The name of a `LIST` of type `list`.
    pub const fn list(list: &[u8; 4]) -> Self {
        Self {
            tag: Tag::LIST,
            list: Some(Tag(*list)),
        }
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.list {
            Some(list) => write!(f, "{}:{list}", self.tag),
            None => write!(f, "{}", self.tag),
        }
    }
}

/// One chunk, borrowed from the file. A list's body starts after its type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Chunk<'a> {
    /// Its tag and list type.
    pub name: Name,
    /// The body, without the pad byte (and, for a list, without the type).
    pub body: &'a [u8],
}

impl<'a> Chunk<'a> {
    /// The chunks in the body: a list's children, or the strings inside a
    /// wrapper chunk such as `tdsn`.
    ///
    /// # Errors
    /// A diagnostic when a chunk claims more bytes than the body holds, or the
    /// body ends inside a chunk header.
    pub fn chunks(&self) -> Result<Vec<Chunk<'a>>> {
        let mut rest = self.body;
        std::iter::from_fn(|| (!rest.is_empty()).then(|| split(&mut rest))).collect()
    }
}

/// Reads the `RIFX` header and returns the root `Egg!` list. The XMP packet
/// after the root chunk is not part of the tree.
///
/// # Errors
/// A diagnostic when the file does not start with `RIFX` and `Egg!`, or the
/// root claims more bytes than the file holds.
pub(crate) fn read(file: &[u8]) -> Result<Chunk<'_>> {
    let not_rifx = || {
        Diagnostic::new(Kind::Invalid, "the file is not a RIFX project")
            .expected("a file that starts with `RIFX` and `Egg!`")
    };
    if !file.starts_with(&Tag::RIFX.0) {
        return Err(not_rifx());
    }
    let mut rest = file;
    let root = split(&mut rest).map_err(|d| d.at("RIFX"))?;
    match root.body.split_first_chunk::<4>() {
        Some((form, body)) if Tag(*form) == Tag::FORM => Ok(Chunk {
            name: Name {
                tag: Tag::RIFX,
                list: Some(Tag::FORM),
            },
            body,
        }),
        _ => Err(not_rifx()),
    }
}

/// Splits the first chunk off `rest`; a missing final pad byte is tolerated.
fn split<'a>(rest: &mut &'a [u8]) -> Result<Chunk<'a>> {
    let header: Header = record::parse(rest, "chunk header", ())?;
    let size = usize::try_from(header.size).unwrap_or(usize::MAX);
    let after = rest.get(Header::SIZE..).unwrap_or_default();
    let body = after.get(..size).ok_or_else(|| {
        Diagnostic::new(
            Kind::Invalid,
            format!(
                "chunk {:?} claims {size} bytes but only {} remain",
                header.tag,
                after.len()
            ),
        )
        .expected("a chunk that ends inside its list")
    })?;
    *rest = after.get(size + size % 2..).unwrap_or_default();
    let name = Name {
        tag: header.tag,
        list: None,
    };
    if header.tag != Tag::LIST {
        return Ok(Chunk { name, body });
    }
    let (list, body) = body.split_first_chunk::<4>().ok_or_else(|| {
        Diagnostic::new(Kind::Invalid, "a `LIST` chunk is shorter than its type")
            .expected("a four-character list type")
    })?;
    Ok(Chunk {
        name: Name::list(list),
        body,
    })
}

#[cfg(test)]
#[path = "../tests/unit/rifx.rs"]
mod tests;
