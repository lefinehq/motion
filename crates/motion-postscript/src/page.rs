//! What a program draws: a list of painted items over one box, in the
//! vector form the rasteriser takes (kurbo paths, a fill rule, a stroke
//! style, an sRGB colour and a clip), plus notes on what was not drawn.

use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect, Size, Stroke};

/// How the inside of a path is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    /// Inside where the path winds around a point a non-zero number of
    /// times (`fill`, `clip`).
    NonZero,
    /// Inside where it crosses an odd number of times (`eofill`, `eoclip`).
    EvenOdd,
}

/// An opaque colour in sRGB, each channel from 0 to 1, gamma-encoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
}

impl Rgb {
    /// Black.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0);

    /// The colour of channels `r`, `g`, `b`, each clamped to 0–1.
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
        }
    }

    /// The colour as 8-bit channels, rounded.
    pub fn to_u8(self) -> [u8; 3] {
        // Clamped to 0–1, so the products fit a byte.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let channel = |v: f32| (v * 255.0).round() as u8;
        [channel(self.r), channel(self.g), channel(self.b)]
    }
}

/// One clip: the inside of `path` by `rule`, inside its parent's.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// The clip path, in page space.
    pub path: BezPath,
    /// How the inside of the path is decided.
    pub rule: FillRule,
    /// The clip this one narrows, by index into [`Page::clips`].
    pub parent: Option<usize>,
}

/// A picture the program painted: rows of straight-alpha sRGB pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    /// The width in samples.
    pub width: u32,
    /// The height in samples.
    pub height: u32,
    /// `width × height` pixels of four bytes, red, green, blue, alpha, row
    /// by row from the first row of the image's data.
    pub pixels: Vec<u8>,
}

/// One painted item.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A filled path, in page space.
    Fill {
        /// The outline, in page space.
        path: BezPath,
        /// How its inside is decided.
        rule: FillRule,
        /// The colour it is filled with.
        colour: Rgb,
        /// The clip it is drawn inside, by index into [`Page::clips`].
        clip: Option<usize>,
    },
    /// A stroked path. The path and the style are in the user space the
    /// stroke was made in, and `transform` takes that space to the page, so
    /// a pen that the matrix stretches comes out stretched.
    Stroke {
        /// The path, in the stroke's user space.
        path: BezPath,
        /// The pen, in the same space. A width of zero asks for the
        /// thinnest line the device draws.
        style: Stroke,
        /// From the stroke's user space to page space.
        transform: Affine,
        /// The colour of the line.
        colour: Rgb,
        /// The clip it is drawn inside.
        clip: Option<usize>,
    },
    /// A picture. `transform` takes the image's own space, one unit per
    /// sample with the first row at `y = 0`, to the page.
    Image {
        /// The samples, as sRGB pixels.
        raster: Arc<Raster>,
        /// From image space to page space.
        transform: Affine,
        /// Whether the program asked for the samples to be smoothed.
        interpolate: bool,
        /// The clip it is drawn inside.
        clip: Option<usize>,
    },
}

/// What a note says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// A feature the program used that is not drawn.
    Unsupported,
    /// An error that ended the program early; what it drew so far stays.
    Error,
    /// A safety limit that ended the program early.
    Limit,
}

/// A note on a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// What sort of note it is.
    pub kind: NoteKind,
    /// What happened, for a report.
    pub message: String,
    /// How often it happened.
    pub count: u64,
}

/// Everything a program drew, over its bounding box.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// The bounding box the file declares, in the program's default user
    /// space (points, y up): the whole-point `%%BoundingBox`, which is the
    /// size a project gives the footage, or the precise box when the file
    /// gives only that.
    pub bounding_box: Rect,
    /// The page's size: the bounding box's. Page space has its origin at
    /// the box's top-left corner and y pointing down, one unit per point.
    pub size: Size,
    /// Where the art lies, in page space: the precise
    /// `%%HiResBoundingBox`, when the file gives one.
    pub art: Option<Rect>,
    /// The painted items, bottom first.
    pub items: Vec<Item>,
    /// The clips items refer to.
    pub clips: Vec<Clip>,
    /// What was not drawn, and why the program stopped if it did not end.
    pub notes: Vec<Note>,
    /// How many operations the program took.
    pub operations: u64,
}

/// Collects items as the program paints.
#[derive(Debug)]
pub(crate) struct Builder {
    pub bounding_box: Rect,
    pub items: Vec<Item>,
    pub clips: Vec<Clip>,
    /// Path elements stored so far, against the output limit.
    pub elements: usize,
    pub max_elements: usize,
}

impl Builder {
    pub fn new(bounding_box: Rect, max_elements: usize) -> Self {
        Self {
            bounding_box,
            items: Vec::new(),
            clips: Vec::new(),
            elements: 0,
            max_elements,
        }
    }

    /// The matrix from default user space to page space.
    pub fn initial_matrix(&self) -> Affine {
        let b = self.bounding_box;
        Affine::new([1.0, 0.0, 0.0, -1.0, -b.x0, b.y1])
    }

    /// Counts `n` more stored path elements; false past the limit.
    pub fn room(&mut self, n: usize) -> bool {
        self.elements = self.elements.saturating_add(n.max(1));
        self.elements <= self.max_elements
    }
}
