//! A PostScript language interpreter for EPS footage ([D71]): it runs the
//! program inside an EPS file and returns what it painted as vector
//! paths, fills, strokes and pictures over the file's bounding box, for
//! our own rasteriser to draw at any size.
//!
//! The interpreter follows the published language definition: a scanner,
//! the operand, dictionary, execution and graphics-state stacks, the
//! control and composite-object operators, files and decoding filters,
//! the path, paint, colour and image operators, and as much of the second
//! language level as the procedure sets of EPS writers use. Text is not
//! drawn, and what a program uses that is not drawn comes back as
//! [`Note`]s, never silently.
//!
//! A run is deterministic and bounded: an operation budget, a memory
//! budget, depth limits and output limits ([`Limits`]) end a hostile or
//! endless program, and an error the program does not catch ends it with
//! a note, keeping what it drew. Nothing here panics on input, and the
//! crate holds no unsafe code.
//!
//! [D71]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d71-eps-footage-is-drawn-by-our-own-postscript-interpreter

#![forbid(unsafe_code)]

mod colour;
mod eps;
mod filter;
mod graphics;
mod interp;
mod object;
mod ops;
mod page;
mod scan;
mod vm;

use std::fmt;

use kurbo::Rect;

pub use page::{Clip, FillRule, Item, Note, NoteKind, Page, Raster, Rgb};

use interp::{Fault, Halt, Interp};
use page::Builder;

/// The bounds a run keeps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most objects the interpreter executes, counting the work inside
    /// operators that loop over data.
    pub operations: u64,
    /// The most memory a program allocates, in units of one string byte,
    /// one array slot or one dictionary entry.
    pub memory: usize,
    /// The deepest the execution stack grows.
    pub depth: usize,
    /// How deep operators that run procedures (tint transforms, data
    /// sources) nest.
    pub nesting: u32,
    /// The most operands on the operand stack.
    pub operands: usize,
    /// The deepest the dictionary stack grows.
    pub dictionaries: usize,
    /// The deepest `gsave` and `save` nest.
    pub gsaves: usize,
    /// The most elements the current path holds.
    pub path_elements: usize,
    /// The most items a page holds.
    pub items: usize,
    /// The most path elements the page's items and clips hold together.
    pub stored_elements: usize,
    /// The most pixels one image has.
    pub image_pixels: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            operations: 100_000_000,
            memory: 64 << 20,
            depth: 4096,
            nesting: 32,
            operands: 65_536,
            dictionaries: 1024,
            gsaves: 1024,
            path_elements: 1 << 20,
            items: 1 << 18,
            stored_elements: 1 << 22,
            image_pixels: 1 << 24,
        }
    }
}

/// Why a file could not be read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The file is neither a PostScript program nor a binary EPS file.
    NotPostScript,
    /// A binary EPS header points outside the file.
    Truncated,
    /// The program declares no usable bounding box.
    NoBoundingBox,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotPostScript => "not an EPS or PostScript file",
            Self::Truncated => "a binary EPS header that points outside the file",
            Self::NoBoundingBox => "an EPS file without a bounding box",
        })
    }
}

impl std::error::Error for Error {}

/// Runs the EPS file `bytes` with the default limits.
pub fn read(bytes: &[u8]) -> Result<Page, Error> {
    read_with(bytes, &Limits::default())
}

/// Runs the EPS file `bytes` within `limits`.
pub fn read_with(bytes: &[u8], limits: &Limits) -> Result<Page, Error> {
    let program = eps::program(bytes)?;
    let declared = eps::bounding_box(program)?;
    let bounding_box = declared.page;
    // The precise box, in page space.
    let art = declared.art.map(|a| {
        Rect::new(
            a.x0 - bounding_box.x0,
            bounding_box.y1 - a.y1,
            a.x1 - bounding_box.x0,
            bounding_box.y1 - a.y0,
        )
    });
    let builder = Builder::new(bounding_box, limits.stored_elements);
    let mut notes = Vec::new();
    let mut interp = match Interp::new(program, *limits, builder) {
        Ok(interp) => interp,
        Err(_) => {
            return Ok(Page {
                bounding_box,
                size: bounding_box.size(),
                art,
                items: Vec::new(),
                clips: Vec::new(),
                notes: vec![Note {
                    kind: NoteKind::Limit,
                    message: "the memory limit is too small to start".into(),
                    count: 1,
                }],
                operations: 0,
            });
        }
    };
    let result = interp.run();
    if let Err(fault) = result {
        let note = match fault {
            Fault::Halt(Halt::Quit) => None,
            Fault::Halt(Halt::Error { name, command }) => Some(Note {
                kind: NoteKind::Error,
                message: format!("the program stopped on `{name}` in `{command}`"),
                count: 1,
            }),
            Fault::Halt(halt) => Some(Note {
                kind: NoteKind::Limit,
                message: match halt {
                    Halt::Budget => "the operation budget ran out",
                    Halt::Depth => "procedures nested deeper than the limit",
                    Halt::Memory => "the memory budget ran out",
                    _ => "the drawing grew past the output limit",
                }
                .into(),
                count: 1,
            }),
            Fault::Error(error) => Some(Note {
                kind: NoteKind::Error,
                message: format!("the program stopped on `{}`", error.name()),
                count: 1,
            }),
            Fault::Stop => None,
        };
        notes.extend(note);
    }
    for (feature, count) in std::mem::take(&mut interp.unsupported) {
        notes.push(Note {
            kind: NoteKind::Unsupported,
            message: feature,
            count,
        });
    }
    let Builder { items, clips, .. } = interp.out;
    Ok(Page {
        bounding_box,
        size: bounding_box.size(),
        art,
        items,
        clips,
        notes,
        operations: interp.steps,
    })
}
