//! A drawing: one page of vector footage as display-list nodes, read once
//! and drawn at whatever size a frame needs.

use std::fmt;
use std::sync::{Mutex, PoisonError};

use nakkash_format::{Diagnostic, Kind, Result};
use nakkash_raster::kurbo::{Affine, Rect, Size};
use nakkash_raster::{Blending, Clip, Frame, Layer, Node, Picture, RenderOptions};

use crate::canvas::Canvas;

/// The most pixels a side a drawing is drawn into a picture at, as the
/// rasteriser's buffers allow.
const MAX_SIDE: u32 = 65535;

/// One page of vector footage: its size, what it paints in its own space,
/// and what its reader could not draw.
///
/// Its space is the page's: in the page's units (points for PDF), y down,
/// from the page's top-left corner at (0, 0) to its size. Its nodes draw in
/// that space; [`Drawing::placed`] lays a part of it over a box, and
/// [`Drawing::picture`] draws that part into pixels once per size asked.
pub struct Drawing {
    /// The name of the format it was read from (`PDF`).
    format: &'static str,
    /// The page's width and height.
    size: Size,
    /// The bounds of its artwork, when the file records them.
    art: Option<Rect>,
    /// What it paints, back to front.
    nodes: Vec<Node>,
    /// What its reader could not draw, sorted, each once.
    unsupported: Vec<String>,
    /// The pictures drawn of it so far, by the part drawn and its size.
    pictures: Mutex<Vec<(Drawn, Picture)>>,
}

/// A part of a drawing drawn at a size: the part's corners, bit for bit,
/// and the size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Drawn {
    region: [u64; 4],
    width: u32,
    height: u32,
}

impl Drawing {
    /// A drawing of a page of `size`, from what a reader painted on
    /// `canvas`. `format` names the file format, `art` the bounds of the
    /// artwork when the file records them.
    pub fn new(format: &'static str, size: Size, art: Option<Rect>, canvas: Canvas) -> Drawing {
        let (nodes, unsupported) = canvas.finish();
        Drawing {
            format,
            size,
            art,
            nodes,
            unsupported,
            pictures: Mutex::new(Vec::new()),
        }
    }

    /// The name of the format it was read from, as reports say it (`PDF`).
    pub fn format(&self) -> &'static str {
        self.format
    }

    /// The page's width and height, in its units.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The page: from (0, 0) to its size.
    pub fn bounds(&self) -> Rect {
        self.size.to_rect()
    }

    /// The bounds of its artwork, in its space, when the file records them.
    pub fn art(&self) -> Option<Rect> {
        self.art
    }

    /// What it paints, back to front, in its own space.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// What its reader could not draw, as the import reports it: sorted,
    /// each once.
    pub fn unsupported(&self) -> &[String] {
        &self.unsupported
    }

    /// The part `region` of it, in its own space, stretched over `frame`, as
    /// one layer: nothing outside `region` shows. As vectors, it stays
    /// sharp under whatever transform its parent gives it.
    pub fn placed(&self, region: Rect, frame: Rect) -> Node {
        Node::Layer(Layer {
            transform: fit(region, frame),
            clip: Some(Clip::Rect(region)),
            ..Layer::new(self.nodes.clone())
        })
    }

    /// The part `region` of it drawn into a `width` by `height` picture,
    /// stretched to fill it. Drawn once per region and size, with the
    /// portable rasteriser, so it is the same on every machine; asked
    /// again, the same picture is shared.
    ///
    /// # Errors
    /// An out-of-range diagnostic for a size of zero or above 65535 pixels
    /// a side, or a region with no area.
    pub fn picture(&self, region: Rect, width: u32, height: u32) -> Result<Picture> {
        if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("a {width}x{height} picture of vector footage"),
            )
            .expected("1 to 65535 pixels a side"));
        }
        if !(region.width() > 0.0 && region.height() > 0.0) {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!(
                    "a part of vector footage {} by {} wide",
                    region.width(),
                    region.height()
                ),
            )
            .expected("a part with an area"));
        }
        let key = Drawn {
            region: [region.x0, region.y0, region.x1, region.y1].map(f64::to_bits),
            width,
            height,
        };
        let drawn = self
            .pictures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(drawn, _)| *drawn == key)
            .map(|(_, picture)| picture.clone());
        if let Some(picture) = drawn {
            return Ok(picture);
        }
        let frame = Frame {
            background: None,
            blending: Blending::Gamma,
            nodes: vec![self.placed(
                region,
                Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            )],
        };
        let image = nakkash_raster::render_with(&frame, width, height, RenderOptions::GOLDEN)?;
        let picture = Picture::from_image(&image)?;
        self.pictures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((key, picture.clone()));
        Ok(picture)
    }
}

/// The map that stretches `from` over `to`, corner to corner.
fn fit(from: Rect, to: Rect) -> Affine {
    let (sx, sy) = (to.width() / from.width(), to.height() / from.height());
    Affine::new([sx, 0.0, 0.0, sy, to.x0 - from.x0 * sx, to.y0 - from.y0 * sy])
}

/// Its format, size and art bounds, how many nodes it has and what it
/// could not draw; not the nodes themselves.
impl fmt::Debug for Drawing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Drawing")
            .field("format", &self.format)
            .field("size", &self.size)
            .field("art", &self.art)
            .field("nodes", &self.nodes.len())
            .field("unsupported", &self.unsupported)
            .finish()
    }
}

#[cfg(test)]
#[path = "../tests/unit/drawing.rs"]
mod tests;
