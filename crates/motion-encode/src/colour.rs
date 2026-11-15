//! From drawn pixels to the planes a codec takes: BT.709 Y′CbCr in limited
//! range, chroma halved both ways (4:2:0), which is what players assume for
//! HD video. Every codec gets its planes from here, so they differ only in
//! how they compress them.

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
/// # Panics
/// If `rgba` does not hold `width * height * 4` bytes; the caller checks.
pub fn to_planes(rgba: &[u8], width: u32, height: u32) -> Planes {
    let (w, h) = (width as usize, height as usize);
    assert_eq!(rgba.len(), w * h * 4, "four bytes per pixel");
    let pixel = |x: usize, y: usize| -> [f32; 3] {
        let at = (y * w + x) * 4;
        let alpha = f32::from(rgba[at + 3]) / 255.0;
        [0, 1, 2].map(|c| f32::from(rgba[at + c]) / 255.0 * alpha)
    };
    let mut y_plane = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            y_plane.push(quantise(16.0 + 219.0 * luma(pixel(x, y))));
        }
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cb = Vec::with_capacity(cw * ch);
    let mut cr = Vec::with_capacity(cw * ch);
    for by in 0..ch {
        for bx in 0..cw {
            let mut sum = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in (by * 2)..(by * 2 + 2).min(h) {
                for x in (bx * 2)..(bx * 2 + 2).min(w) {
                    let p = pixel(x, y);
                    sum = [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]];
                    count += 1.0;
                }
            }
            let mean = sum.map(|s| s / count);
            let (b, r) = chroma(mean);
            cb.push(quantise(128.0 + 224.0 * b));
            cr.push(quantise(128.0 + 224.0 * r));
        }
    }
    Planes {
        width,
        height,
        y: y_plane,
        cb,
        cr,
    }
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
