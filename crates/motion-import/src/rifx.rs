//! The chunk tree of a RIFX file: the `RIFX` header and its `Egg!` form,
//! chunks, and `LIST`s, with bodies padded to even sizes
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#file-structure)).
//!
//! Generic: the only tag it knows is `LIST`. Lists are read lazily, one level
//! at a time, so a list that holds raw bytes instead of chunks (a text
//! document's `LIST:btdk`) is never misread unless `decode` asks for it.
//!
//! [`write`] is the way back: the file with some chunk bodies changed, every
//! other byte as it was, and the sizes of the lists around a changed body
//! made right ([D45]).
//!
//! [D45]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d45-export-patches-the-project-it-came-from

use std::fmt;
use std::ops::Range;

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

/// A change to a file: the bytes at `range`, which lie inside one chunk's
/// body, become `bytes`, which may be longer or shorter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// The bytes replaced, as offsets into the file.
    pub range: Range<usize>,
    /// What replaces them.
    pub bytes: Vec<u8>,
}

/// One chunk's place in a file: where its header starts, where its body
/// starts and ends, and where the next chunk starts.
#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    body: usize,
    end: usize,
    next: usize,
}

/// `file` with `patches` applied. A chunk no patch touches is copied as it
/// is, pad byte included; a chunk a patch touches gets its new size, and so
/// does every list around it. A list whose body is not a run of chunks
/// (a text document's `LIST:btdk`) is patched as raw bytes. Bytes after the
/// root chunk, such as the XMP packet, are copied as they are.
///
/// # Errors
/// A diagnostic when the file is not a RIFX project, patches overlap, or a
/// patch does not lie inside one chunk's body.
pub fn write(file: &[u8], patches: &[Patch]) -> Result<Vec<u8>> {
    read(file)?;
    let mut sorted: Vec<&Patch> = patches.iter().collect();
    sorted.sort_by_key(|p| (p.range.start, p.range.end));
    if let Some(pair) = sorted
        .windows(2)
        .find(|w| w[0].range.end > w[1].range.start)
    {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "patches at bytes {:?} and {:?} overlap",
                pair[0].range, pair[1].range
            ),
        ));
    }
    let mut out = Vec::with_capacity(file.len());
    let root = span(file, 0, file.len()).ok_or_else(|| {
        Diagnostic::new(
            Kind::Invalid,
            "the root chunk runs past the end of the file",
        )
    })?;
    let applied = chunk(file, root, &sorted, &mut out)?;
    out.extend_from_slice(file.get(root.next..).unwrap_or_default());
    if applied != sorted.len() {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "{} of {} patches lie outside every chunk's body",
                sorted.len() - applied,
                sorted.len()
            ),
        ));
    }
    Ok(out)
}

/// The chunk whose header starts at `start`, inside a region that ends at
/// `end`; none when it does not fit.
fn span(file: &[u8], start: usize, end: usize) -> Option<Span> {
    let header = file.get(start..start.checked_add(Header::SIZE)?)?;
    let size = usize::try_from(u32::from_be_bytes(header[4..8].try_into().ok()?)).ok()?;
    let body = start + Header::SIZE;
    let body_end = body.checked_add(size)?;
    (body_end <= end).then(|| Span {
        start,
        body,
        end: body_end,
        next: (body_end + size % 2).min(end),
    })
}

/// The chunks of the region `start..end`, when it is exactly a run of them.
fn chunks_in(file: &[u8], start: usize, end: usize) -> Option<Vec<Span>> {
    let mut spans = Vec::new();
    let mut at = start;
    while at < end {
        let span = span(file, at, end)?;
        at = span.next;
        spans.push(span);
    }
    Some(spans)
}

/// Writes the chunk at `span` with the patches inside its body, and returns
/// how many it applied.
fn chunk(file: &[u8], span: Span, patches: &[&Patch], out: &mut Vec<u8>) -> Result<usize> {
    let inside: Vec<&Patch> = patches
        .iter()
        .copied()
        .filter(|p| p.range.start >= span.body && p.range.end <= span.end)
        .collect();
    if inside.is_empty() {
        out.extend_from_slice(&file[span.start..span.next]);
        return Ok(0);
    }
    let tag = &file[span.start..span.start + 4];
    out.extend_from_slice(tag);
    let size_at = out.len();
    out.extend_from_slice(&[0; 4]);
    let list_type = span.body + 4;
    // A list of chunks (the root is one) whose patches each lie inside one
    // child: recurse.
    let listed = tag == Tag::LIST.0 || tag == Tag::RIFX.0;
    let children = (listed && span.end >= list_type)
        .then(|| chunks_in(file, list_type, span.end))
        .flatten()
        .filter(|children| {
            inside.iter().all(|p| {
                children
                    .iter()
                    .any(|c| p.range.start >= c.body && p.range.end <= c.end)
            })
        });
    let applied = match children {
        Some(children) => {
            out.extend_from_slice(&file[span.body..list_type]);
            let mut applied = 0;
            for child in children {
                applied += chunk(file, child, &inside, out)?;
            }
            applied
        }
        None => {
            let mut at = span.body;
            for patch in &inside {
                out.extend_from_slice(&file[at..patch.range.start]);
                out.extend_from_slice(&patch.bytes);
                at = patch.range.end;
            }
            out.extend_from_slice(&file[at..span.end]);
            inside.len()
        }
    };
    let size = out.len() - size_at - 4;
    let size32 = u32::try_from(size)
        .map_err(|_| Diagnostic::new(Kind::Invalid, "a patched chunk is larger than 4 GB"))?;
    out[size_at..size_at + 4].copy_from_slice(&size32.to_be_bytes());
    if size % 2 == 1 {
        // The old pad byte where there was one, else a zero.
        let pad = if span.next > span.end {
            file[span.end]
        } else {
            0
        };
        out.push(pad);
    }
    Ok(applied)
}

#[cfg(test)]
#[path = "../tests/unit/rifx.rs"]
mod tests;
