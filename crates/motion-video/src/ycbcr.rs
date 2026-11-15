//! From decoded Y′CbCr pictures to RGBA: which matrix, range and chroma
//! siting a clip's samples are in, and the conversion.
//!
//! A clip says how its samples are coded in two places, its sample
//! description's `colr` box and the video signal type in its stream; the
//! box wins where both say, as ISO/IEC 14496-12 has it. A clip that says
//! nothing is BT.709 in limited range when it is larger than standard
//! definition and BT.601 when it is not, as players read untagged video
//! ([the fidelity notes](../../../docs/research/2026-11-14-fidelity-first-sample.md)
//! show the reference renders are read so). This is the inverse of what
//! `motion-encode` writes.
//!
//! Only the matrix and the range change the numbers: the primaries and the
//! transfer are those of the pixels the rest of motion draws with, as an
//! unmanaged project keeps footage, so they are read and left alone.

use motion_format::{Diagnostic, Kind, Result};

use crate::Frame;
use crate::avc::Signal;
use crate::container::{FourCc, Track};

/// How a clip's Y′CbCr samples map to RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Colour {
    /// The luma weights of red and blue.
    pub(crate) matrix: Matrix,
    /// Whether samples use the full range of their bits.
    pub(crate) full_range: bool,
    /// Where each chroma sample sits among the luma samples.
    pub(crate) site: Site,
}

/// The matrix coefficients this crate converts, by the luma weights of red
/// and blue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Matrix {
    /// BT.709 (H.273 number 1), HD video.
    Bt709,
    /// BT.601 (5 and 6), standard-definition video.
    Bt601,
    /// The FCC's (4).
    Fcc,
    /// SMPTE 240M (7).
    Smpte240,
    /// BT.2020 without constant luminance (9).
    Bt2020,
}

impl Matrix {
    /// The matrix H.273 numbers `code`: `Ok(None)` when it says nothing,
    /// an error for one this crate does not convert.
    fn of(code: u16) -> Result<Option<Matrix>> {
        Ok(Some(match code {
            1 => Matrix::Bt709,
            4 => Matrix::Fcc,
            5 | 6 => Matrix::Bt601,
            7 => Matrix::Smpte240,
            9 => Matrix::Bt2020,
            // Unspecified and reserved.
            2 | 3 => return Ok(None),
            other => {
                let name = match other {
                    0 => "identity (RGB)",
                    8 => "YCgCo",
                    10 => "BT.2020 constant luminance",
                    11 => "SMPTE ST 2085",
                    12 | 13 => "chromaticity-derived",
                    14 => "ICtCp",
                    _ => "an unknown one",
                };
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("video in the matrix coefficients {other}, {name}"),
                )
                .expected("BT.709, BT.601, BT.2020, SMPTE 240M or FCC Y′CbCr"));
            }
        }))
    }

    /// The luma weights of red and blue.
    fn weights(self) -> (f64, f64) {
        match self {
            Matrix::Bt709 => (0.2126, 0.0722),
            Matrix::Bt601 => (0.299, 0.114),
            Matrix::Fcc => (0.30, 0.11),
            Matrix::Smpte240 => (0.212, 0.087),
            Matrix::Bt2020 => (0.2627, 0.0593),
        }
    }
}

/// Where a chroma sample sits, in half luma samples right of and below the
/// top left luma sample of the two by two it covers: `x` 0 or 1, `y` 0 to 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Site {
    pub(crate) x: u32,
    pub(crate) y: u32,
}

impl Site {
    /// The siting H.273 numbers `code`, 0 (left, between rows), the default,
    /// for any it does not number.
    fn of(code: u32) -> Site {
        let (x, y) = match code {
            1 => (1, 1),
            2 => (0, 0),
            3 => (1, 0),
            4 => (0, 2),
            5 => (1, 2),
            _ => (0, 1),
        };
        Site { x, y }
    }
}

/// What a `colr` box says: its colour description, and its range when it
/// has one (`nclx` does, QuickTime's `nclc` does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tagged {
    matrix: u16,
    full_range: Option<bool>,
}

/// The first `colr` box among `extensions` with a colour description;
/// boxes holding an ICC profile say nothing about the matrix.
fn tagged(extensions: &[(FourCc, Vec<u8>)]) -> Option<Tagged> {
    extensions
        .iter()
        .filter(|(kind, _)| kind == b"colr")
        .find_map(|(_, body)| {
            let kind = body.get(0..4)?;
            let matrix = u16::from_be_bytes([*body.get(8)?, *body.get(9)?]);
            match kind {
                b"nclx" => Some(Tagged {
                    matrix,
                    full_range: Some(body.get(10)? & 0x80 != 0),
                }),
                b"nclc" => Some(Tagged {
                    matrix,
                    full_range: None,
                }),
                _ => None,
            }
        })
}

impl Colour {
    /// The colour of `track`'s samples, from its `colr` box, then its
    /// stream's `signal` type and `site`, then its size.
    ///
    /// # Errors
    /// An invalid-value diagnostic for matrix coefficients this crate does
    /// not convert.
    pub(crate) fn of(track: &Track, signal: Option<Signal>, site: Option<u32>) -> Result<Colour> {
        let tagged = tagged(&track.extensions);
        let from_box = match tagged {
            Some(t) => Matrix::of(t.matrix)?,
            None => None,
        };
        let from_stream = match signal.and_then(|s| s.description) {
            Some([_, _, matrix]) => Matrix::of(u16::from(matrix))?,
            None => None,
        };
        let standard = track.width <= 720 && track.height <= 576;
        let matrix = from_box.or(from_stream).unwrap_or(match standard {
            true => Matrix::Bt601,
            false => Matrix::Bt709,
        });
        let full_range = tagged
            .and_then(|t| t.full_range)
            .or(signal.map(|s| s.full_range))
            .unwrap_or(false);
        Ok(Colour {
            matrix,
            full_range,
            site: Site::of(site.unwrap_or(0)),
        })
    }
}

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
        let (kr, kb) = colour.matrix.weights();
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
