//! From drawn pixels to the planes a codec takes: BT.709 Y′CbCr in limited
//! range, chroma halved both ways (4:2:0), which is what players assume for
//! HD video. Every codec gets its planes from here, so they differ only in
//! how they compress them.

use std::sync::LazyLock;
use std::thread;

/// The luma weights of BT.709's red, green and blue.
const KR: f32 = 0.2126;
const KB: f32 = 0.0722;
const KG: f32 = 1.0 - KR - KB;

/// One frame as three 8-bit planes, rows top to bottom with no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planes {
    /// Width of the luma plane, in pixels.
    pub width: u32,
    /// Height of the luma plane, in pixels.
    pub height: u32,
    /// Luma, `width * height` samples.
    pub y: Vec<u8>,
    /// Blue-difference chroma, one sample per 2x2 block.
    pub cb: Vec<u8>,
    /// Red-difference chroma, one sample per 2x2 block.
    pub cr: Vec<u8>,
}

impl Planes {
    /// The width of each chroma plane: half the luma's, rounded up.
    pub fn chroma_width(&self) -> u32 {
        self.width.div_ceil(2)
    }

    /// The height of each chroma plane: half the luma's, rounded up.
    pub fn chroma_height(&self) -> u32 {
        self.height.div_ceil(2)
    }
}

/// The planes of `rgba`, `width` by `height` straight-alpha RGBA8 pixels.
/// A pixel that is not opaque is drawn over black, as a player without
/// alpha would show it. Chroma averages each 2x2 block; a block cut by the
/// right or bottom edge averages the pixels it has.
///
/// Each pixel is read once, and bands of rows are converted on threads of
/// their own, as many as the process works on
/// ([`nakkash_machine::threads`]); every sample is the same as a
/// pixel-by-pixel pass gives.
///
/// # Panics
/// If `rgba` does not hold `width * height * 4` bytes; the caller checks.
pub fn to_planes(rgba: &[u8], width: u32, height: u32) -> Planes {
    let (w, h) = (width as usize, height as usize);
    assert_eq!(rgba.len(), w * h * 4, "four bytes per pixel");
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut y = vec![0; w * h];
    let mut cb = vec![0; cw * ch];
    let mut cr = vec![0; cw * ch];
    // Whole blocks rows per band, so a band's chroma needs no other band.
    let workers = nakkash_machine::threads();
    let blocks = ch.div_ceil(workers.min(ch.div_ceil(BAND_BLOCK_ROWS)).max(1));
    thread::scope(|scope| {
        let bands = y
            .chunks_mut(blocks * 2 * w)
            .zip(cb.chunks_mut(blocks * cw))
            .zip(cr.chunks_mut(blocks * cw));
        for (band, ((y, cb), cr)) in bands.enumerate() {
            let first = band * blocks * 2;
            let rows = &rgba[first * w * 4..((first + blocks * 2).min(h)) * w * 4];
            scope.spawn(move || convert_band(rows, w, y, cb, cr));
        }
    });
    Planes {
        width,
        height,
        y,
        cb,
        cr,
    }
}

/// The fewest block rows (pairs of pixel rows) a thread converts.
const BAND_BLOCK_ROWS: usize = 16;

/// Each 8-bit level over 255, as `f32`.
static UNIT: LazyLock<[f32; 256]> = LazyLock::new(|| std::array::from_fn(|v| v as f32 / 255.0));

/// Converts `rgba`, whole rows `w` pixels wide starting on an even row,
/// into its luma rows and chroma rows.
fn convert_band(rgba: &[u8], w: usize, y_out: &mut [u8], cb_out: &mut [u8], cr_out: &mut [u8]) {
    let unit = &*UNIT;
    let h = rgba.len() / (w * 4);
    let cw = w.div_ceil(2);
    // The block row's pixels, premultiplied, read once for luma and chroma.
    let mut pixels = vec![[0.0f32; 3]; 2 * w];
    for by in 0..h.div_ceil(2) {
        let rows = (by * 2)..(by * 2 + 2).min(h);
        for (r, y) in rows.clone().enumerate() {
            for x in 0..w {
                let at = (y * w + x) * 4;
                let alpha = unit[usize::from(rgba[at + 3])];
                let p = [0, 1, 2].map(|c| unit[usize::from(rgba[at + c])] * alpha);
                pixels[r * w + x] = p;
                y_out[y * w + x] = quantise(16.0 + 219.0 * luma(p));
            }
        }
        for bx in 0..cw {
            let mut sum = [0.0f32; 3];
            let mut count = 0.0f32;
            for r in 0..rows.len() {
                for x in (bx * 2)..(bx * 2 + 2).min(w) {
                    let p = pixels[r * w + x];
                    sum = [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]];
                    count += 1.0;
                }
            }
            let mean = sum.map(|s| s / count);
            let (b, r) = chroma(mean);
            cb_out[by * cw + bx] = quantise(128.0 + 224.0 * b);
            cr_out[by * cw + bx] = quantise(128.0 + 224.0 * r);
        }
    }
}

/// One frame as 16-bit 4:4:4 Y′CbCr with alpha, for codecs that keep alpha:
/// per pixel alpha, luma, blue and red difference, in that order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ayuv16 {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Four samples a pixel, rows top to bottom with no padding.
    pub samples: Vec<u16>,
}

/// The 16-bit samples of `rgba`, `width` by `height` straight-alpha RGBA8
/// pixels. Alpha stays straight and in full range; colour is BT.709 in
/// limited range scaled to 16 bits, each pixel's own, not drawn over black,
/// so a clear area keeps the colour under it.
///
/// # Panics
/// If `rgba` does not hold `width * height * 4` bytes; the caller checks.
pub fn to_ayuv16(rgba: &[u8], width: u32, height: u32) -> Ayuv16 {
    assert_eq!(
        rgba.len(),
        width as usize * height as usize * 4,
        "four bytes per pixel"
    );
    let samples = rgba
        .chunks_exact(4)
        .flat_map(|pixel| {
            let rgb = [0, 1, 2].map(|c| f32::from(pixel[c]) / 255.0);
            let (b, r) = chroma(rgb);
            [
                u16::from(pixel[3]) * 257,
                quantise16(16.0 + 219.0 * luma(rgb)),
                quantise16(128.0 + 224.0 * b),
                quantise16(128.0 + 224.0 * r),
            ]
        })
        .collect();
    Ayuv16 {
        width,
        height,
        samples,
    }
}

/// An 8-bit level, `value`, as a 16-bit sample: 235 becomes 60160.
fn quantise16(value: f32) -> u16 {
    (value * 256.0).round().clamp(0.0, 65535.0) as u16
}

/// Luma, 0 to 1, of gamma-encoded `[r, g, b]` from 0 to 1.
fn luma([r, g, b]: [f32; 3]) -> f32 {
    KR * r + KG * g + KB * b
}

/// Blue and red difference, each -0.5 to 0.5, of `[r, g, b]`.
fn chroma(rgb: [f32; 3]) -> (f32, f32) {
    let y = luma(rgb);
    (
        (rgb[2] - y) / (2.0 * (1.0 - KB)),
        (rgb[0] - y) / (2.0 * (1.0 - KR)),
    )
}

/// `value` rounded into a sample.
fn quantise(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
#[path = "../tests/unit/colour.rs"]
mod tests;
