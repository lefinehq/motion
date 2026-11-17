//! Vector footage ([D70]): files whose pictures are outlines and paints,
//! not pixels, read into the rasteriser's own display list and drawn by
//! it, at whatever size a frame needs.
//!
//! - **Formats.** Each format is a row of [`FORMATS`], recognised by how
//!   its files start, never by a file name: PDF, and so the `.ai` files
//!   that are PDF inside, and EPS, whose PostScript program our own
//!   interpreter runs ([D71]). A reader for another format adds a row.
//! - **One drawing model.** A reader interprets its file's operators and
//!   paints on a [`Canvas`]: fills, strokes and pictures, inside clips and
//!   transparency groups, in the page's own space. The canvas turns them
//!   into [`motion_raster::Node`]s, so every format draws the same way.
//! - **A [`Drawing`]** holds one page: its size, its art bounds, its nodes
//!   and what its reader could not draw. It is laid over a box as vectors
//!   ([`Drawing::placed`]) or drawn into a picture once per size
//!   ([`Drawing::picture`]).
//! - **What is not drawn is said.** Whatever a reader meets and cannot
//!   draw it names on the canvas; [`Drawing::unsupported`] lists it, and
//!   the import reports it on each layer that shows the file.
//!
//! [D70]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines
//! [D71]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d71-eps-footage-is-drawn-by-our-own-postscript-interpreter

mod canvas;
mod drawing;
mod eps;
mod pdf;

pub use canvas::{Canvas, Group};
pub use drawing::Drawing;

use motion_format::{Diagnostic, Kind, Result};

/// A vector file format a [`Drawing`] is read from.
#[derive(Debug, Clone, Copy)]
pub struct Format {
    /// Its name, as reports say it (`PDF`).
    pub name: &'static str,
    /// Whether a file's first bytes are this format's. At most
    /// [`HEAD`] bytes are given.
    pub recognise: fn(&[u8]) -> bool,
    /// Reads a whole file of it.
    pub read: fn(&[u8]) -> Result<Drawing>,
}

/// How many of a file's first bytes recognising its format reads.
pub const HEAD: usize = 1024;

/// The vector formats, recognised by their files' first bytes.
pub const FORMATS: &[Format] = &[
    Format {
        name: "PDF",
        recognise: pdf::recognise,
        read: pdf::read,
    },
    Format {
        name: "EPS",
        recognise: eps::recognise,
        read: eps::read,
    },
];

/// The vector format of a file starting with `head`, or `None` for one in
/// no vector format: a raster picture, say. `head` may be the whole file
/// or only its first [`HEAD`] bytes.
pub fn recognise(head: &[u8]) -> Option<&'static Format> {
    let head = &head[..head.len().min(HEAD)];
    FORMATS.iter().find(|format| (format.recognise)(head))
}

/// Reads a vector file's bytes, its format recognised by how they start.
///
/// # Errors
/// An invalid-value diagnostic for bytes in no vector format, or the
/// format's reader's when the file cannot be read.
pub fn read(bytes: &[u8]) -> Result<Drawing> {
    let format = recognise(bytes).ok_or_else(|| {
        let names: Vec<&str> = FORMATS.iter().map(|format| format.name).collect();
        Diagnostic::new(Kind::Invalid, "a file of no vector format")
            .expected(format!("a {} file", names.join(" or ")))
    })?;
    (format.read)(bytes)
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/sample.rs"]
mod sample;
