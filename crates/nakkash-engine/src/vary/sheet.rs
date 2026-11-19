//! Sheets of frames ([D78]): one row per scene, its frames spread evenly
//! over its length, each drawn small at one height whatever its aspect, on
//! a dark ground.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use nakkash_format::Result;
use nakkash_raster::Image;

use crate::session::Session;

/// The space between frames and around them, in px.
const GAP: u32 = 8;

/// The ground the frames sit on.
const GROUND: [u8; 3] = [24, 24, 28];

/// How a sheet is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sheet {
    /// Frames per row.
    pub columns: u32,
    /// The height of each frame, in px.
    pub height: u32,
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet {
            columns: 6,
            height: 240,
        }
    }
}

/// A sheet of `rows`, each scene's frames in one row: the middle frames of
/// equal parts of its length.
///
/// # Errors
/// As [`Session::display_list`], for a frame that cannot be drawn.
pub fn sheet(rows: &[&Session], layout: Sheet) -> Result<Image> {
    let mut drawn: Vec<Vec<Image>> = Vec::with_capacity(rows.len());
    for session in rows {
        let frames = session.frames()?;
        let (_, height) = session.canvas()?;
        let shrink = f64::from(layout.height) / f64::from(height.max(1));
        let n = layout.columns.max(1);
        let length = frames.end.saturating_sub(frames.start).max(1);
        let mut row = Vec::with_capacity(n as usize);
        for k in 0..n {
            // The middle of each of n equal parts: never the first or last
            // frame, which are often a fade from or to black.
            let frame = frames.start + (length * (2 * k + 1)) / (2 * n);
            row.push(crate::gates::small(session, f64::from(frame), shrink)?);
        }
        drawn.push(row);
    }
    let width = drawn
        .iter()
        .map(|row| row.iter().map(|i| i.width() + GAP).sum::<u32>() + GAP)
        .max()
        .unwrap_or(GAP);
    let height = drawn
        .iter()
        .map(|row| row.iter().map(Image::height).max().unwrap_or(0) + GAP)
        .sum::<u32>()
        + GAP;
    let mut pixels = vec![0_u8; (width * height * 4) as usize];
    for px in pixels.chunks_exact_mut(4) {
        px.copy_from_slice(&[GROUND[0], GROUND[1], GROUND[2], 255]);
    }
    let mut y = GAP;
    for row in &drawn {
        let mut x = GAP;
        for image in row {
            blit(&mut pixels, width, image, x, y);
            x += image.width() + GAP;
        }
        y += row.iter().map(Image::height).max().unwrap_or(0) + GAP;
    }
    Image::new(width, height, pixels)
}

/// Draws `image` over `pixels`, a `width`-wide RGBA8 buffer, at `x, y`.
fn blit(pixels: &mut [u8], width: u32, image: &Image, x: u32, y: u32) {
    let source = image.pixels();
    for row in 0..image.height() {
        for col in 0..image.width() {
            let s = ((row * image.width() + col) * 4) as usize;
            let d = (((y + row) * width + x + col) * 4) as usize;
            let a = f64::from(source[s + 3]) / 255.0;
            for c in 0..3 {
                let over = f64::from(source[s + c]) * a + f64::from(pixels[d + c]) * (1.0 - a);
                pixels[d + c] = over.round() as u8;
            }
        }
    }
}
