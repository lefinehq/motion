//! From decoded Y′CbCr planes to RGBA, in the colour [`crate::colour`]
//! gives: the inverse of what `motion-encode` writes, in fixed point, with
//! chroma upsampled linearly from where its samples sit.

use crate::Frame;
use crate::colour::{Colour, Matrix};

/// A decoded picture's planes, 4:2:0, as `T` samples of which the top
/// `bits` of `T`'s width hold the level: luma, then chroma as Cb, Cr pairs
/// half the size both ways, rounded up. A backend whose chroma comes in two
/// planes adds its own layout here.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Planes<'a, T> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) luma: &'a [T],
    pub(crate) luma_stride: usize,
    pub(crate) chroma: &'a [T],
    pub(crate) chroma_stride: usize,
    /// Bits of a level, at the top of each sample.
    pub(crate) bits: u32,
}

/// A sample type: eight or sixteen bits wide.
pub(crate) trait Sample: Copy {
    /// Its width in bits.
    const WIDTH: u32;
    /// Its value.
    fn value(self) -> i32;
}

impl Sample for u8 {
    const WIDTH: u32 = 8;
    fn value(self) -> i32 {
        i32::from(self)
    }
}

impl Sample for u16 {
    const WIDTH: u32 = 16;
    fn value(self) -> i32 {
        i32::from(self)
    }
}

/// The luma weights of red and blue of `matrix`.
fn weights(matrix: Matrix) -> (f64, f64) {
    match matrix {
        Matrix::Bt709 => (0.2126, 0.0722),
        Matrix::Bt601 => (0.299, 0.114),
        Matrix::Fcc => (0.30, 0.11),
        Matrix::Smpte240 => (0.212, 0.087),
        Matrix::Bt2020 => (0.2627, 0.0593),
    }
}

/// Fixed-point fraction bits of the conversion's coefficients.
const FRACTION: u32 = 16;

/// The conversion's coefficients for one colour and depth: each scaled so
/// the sum lands in 8-bit levels after [`FRACTION`] bits.
struct Coefficients {
    luma_offset: i32,
    /// Chroma's zero, times 16 as interpolated chroma is.
    chroma_offset: i32,
    luma: i32,
    red_cr: i32,
    green_cb: i32,
    green_cr: i32,
    blue_cb: i32,
}

impl Coefficients {
    fn new(colour: Colour, bits: u32) -> Coefficients {
        let scale = f64::from(1u32 << (bits - 8));
        let (luma_offset, luma_range, chroma_zero, chroma_range) = match colour.full_range {
            false => (16.0 * scale, 219.0 * scale, 128.0 * scale, 224.0 * scale),
            true => {
                let top = f64::from((1u32 << bits) - 1);
                (0.0, top, f64::from(1u32 << (bits - 1)), top)
            }
        };
        let (kr, kb) = weights(colour.matrix);
        let kg = 1.0 - kr - kb;
        let one = f64::from(1u32 << FRACTION) * 255.0;
        // Interpolated chroma comes as sixteen times its level.
        let chroma = one / chroma_range / 16.0;
        let fixed = |v: f64| v.round() as i32;
        Coefficients {
            luma_offset: fixed(luma_offset),
            chroma_offset: fixed(chroma_zero * 16.0),
            luma: fixed(one / luma_range),
            red_cr: fixed(chroma * 2.0 * (1.0 - kr)),
            green_cb: fixed(chroma * 2.0 * kb * (1.0 - kb) / kg),
            green_cr: fixed(chroma * 2.0 * kr * (1.0 - kr) / kg),
            blue_cb: fixed(chroma * 2.0 * (1.0 - kb)),
        }
    }
}

/// For each output position along one axis, the two chroma samples it
/// falls between and the weight of the second, in quarters: a chroma sample
/// sits `site` half samples past the first of the two it covers.
fn taps(length: usize, chroma: usize, site: u32) -> Vec<(usize, usize, i32)> {
    let last = chroma.saturating_sub(1) as i64;
    (0..length)
        .map(|i| {
            // The position in chroma samples, in quarters.
            let quarter = 2 * i as i64 - i64::from(site);
            let first = quarter.div_euclid(4);
            let weight = quarter.rem_euclid(4) as i32;
            let at = |j: i64| j.clamp(0, last) as usize;
            (at(first), at(first + 1), weight)
        })
        .collect()
}

/// `planes` as an opaque RGBA frame.
pub(crate) fn to_frame<T: Sample>(planes: Planes<'_, T>, colour: Colour) -> Frame {
    let Planes {
        width,
        height,
        luma,
        luma_stride,
        chroma,
        chroma_stride,
        bits,
    } = planes;
    let shift = T::WIDTH - bits;
    let c = Coefficients::new(colour, bits);
    let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
    let columns = taps(width, chroma_width, colour.site.x);
    let rows = taps(height, chroma_height, colour.site.y);
    let mut pixels = vec![255u8; width * height * 4];
    // One output row's chroma, blended between its two chroma rows: four
    // times the level.
    let mut cb = vec![0i32; chroma_width];
    let mut cr = vec![0i32; chroma_width];
    let level = |sample: T| sample.value() >> shift;
    let round = 1i32 << (FRACTION - 1);
    let channel = |sum: i32| ((sum + round) >> FRACTION).clamp(0, 255) as u8;
    for (y, &(top, bottom, weight)) in rows.iter().enumerate() {
        let (top, bottom) = (
            &chroma[top * chroma_stride..][..chroma_width * 2],
            &chroma[bottom * chroma_stride..][..chroma_width * 2],
        );
        for j in 0..chroma_width {
            let blend =
                |k: usize| (4 - weight) * level(top[2 * j + k]) + weight * level(bottom[2 * j + k]);
            cb[j] = blend(0);
            cr[j] = blend(1);
        }
        let luma = &luma[y * luma_stride..][..width];
        let out = &mut pixels[y * width * 4..][..width * 4];
        for ((pixel, &sample), &(left, right, weight)) in
            out.chunks_exact_mut(4).zip(luma).zip(&columns)
        {
            let b = (4 - weight) * cb[left] + weight * cb[right] - c.chroma_offset;
            let r = (4 - weight) * cr[left] + weight * cr[right] - c.chroma_offset;
            let y = c.luma * (level(sample) - c.luma_offset);
            pixel[0] = channel(y + c.red_cr * r);
            pixel[1] = channel(y - c.green_cb * b - c.green_cr * r);
            pixel[2] = channel(y + c.blue_cb * b);
        }
    }
    Frame {
        width: width as u32,
        height: height as u32,
        pixels,
    }
}

#[cfg(test)]
#[path = "../tests/unit/ycbcr.rs"]
mod tests;
