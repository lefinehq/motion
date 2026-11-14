//! The typed project that `decode` builds from a file: items, compositions,
//! layers and their property trees. It derives `Serialize`, so
//! `motion inspect --json` prints it as it is.
//!
//! Times are seconds and frame rates frames per second. Other values keep the
//! file's units (pixels, degrees, opacity and scale as fractions of 1), except
//! colours, whose channels are fractions from 0 to 1.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// A whole `.aep` project.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    /// The file version stored in `head`, which decides some record layouts.
    pub version: Version,
    /// The project's colour depth: 8, 16 or 32 bits per channel, when known.
    pub bits_per_channel: Option<u8>,
    /// Every item. A folder's contents follow the folder and name it as theirs.
    pub items: Vec<Item>,
    /// The chunks the reader passed over, by name, with how often each appears.
    pub skipped: BTreeMap<String, usize>,
}

/// A file version: the major number grows with each release of the desktop tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Version {
    /// The major number, such as 93.
    pub major: u16,
    /// The minor number, which differs between builds of one release.
    pub minor: u16,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// One project item: a folder, a composition or a footage item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    /// The file's id, shared by items and layers, so links survive.
    pub id: u32,
    /// The name shown for the item; for footage without a name of its own,
    /// the solid's, the placeholder's or the file's name.
    pub name: String,
    /// The id of the folder that holds the item, or `None` at the root.
    pub folder: Option<u32>,
    /// What the item is.
    #[serde(flatten)]
    pub kind: ItemKind,
}

/// What an item is, with the settings of its kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemKind {
    /// A folder; its contents name it as their folder.
    Folder,
    /// A composition.
    Composition(Composition),
    /// Footage: a solid, a file or a placeholder.
    Footage(Footage),
}

/// A composition's settings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Composition {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Pixel aspect ratio: 1 for square pixels.
    pub pixel_aspect: f64,
    /// Frames per second.
    pub frame_rate: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// Background colour.
    pub background: Color,
    /// The part of the composition that renders by default.
    pub work_area: Span,
}

/// A time range in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Span {
    /// Where the range starts.
    pub start: f64,
    /// Where the range ends.
    pub end: f64,
}

/// A footage item's media: its size and timing, and where it comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Footage {
    /// Width in pixels; 0 for audio.
    pub width: u32,
    /// Height in pixels; 0 for audio.
    pub height: u32,
    /// Pixel aspect ratio: 1 for square pixels.
    pub pixel_aspect: f64,
    /// Frames per second it plays at; 0 for stills.
    pub frame_rate: f64,
    /// Duration in seconds; 0 for stills and solids.
    pub duration: f64,
    /// Where the pixels come from.
    #[serde(flatten)]
    pub source: Source,
}

/// Where a footage item's pixels come from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Source {
    /// A solid colour.
    Solid {
        /// The colour.
        color: Color,
    },
    /// A file on disk, or the folder of an image sequence.
    File {
        /// The absolute path as stored, or `None` when its record cannot be read.
        path: Option<String>,
    },
    /// A stand-in for missing media.
    Placeholder,
}

/// A colour, each channel a fraction from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Color {
    /// Red.
    pub r: f64,
    /// Green.
    pub g: f64,
    /// Blue.
    pub b: f64,
    /// Alpha.
    pub a: f64,
}
