//! Which matrix, range and chroma siting a clip's Y′CbCr samples are in.
//!
//! A clip says how its samples are coded in two places, its sample
//! description's `colr` box and the video signal type in its stream; the
//! box wins where both say, as ISO/IEC 14496-12 has it. A clip that says
//! nothing is BT.709 in limited range when it is larger than standard
//! definition and BT.601 when it is not, as players read untagged video
//! (the fidelity notes, `docs/research/2026-11-14-fidelity-first-sample.md`,
//! show the reference renders are read so).
//!
//! Only the matrix and the range change the numbers: the primaries and the
//! transfer are those of the pixels the rest of motion draws with, as an
//! unmanaged project keeps footage, so they are read and left alone.

use motion_format::{Diagnostic, Kind, Result};

use crate::avc::Signal;
use crate::container::{FourCc, Track};

/// How a clip's Y′CbCr samples map to RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Colour {
    /// The matrix coefficients.
    pub(crate) matrix: Matrix,
    /// Whether samples use the full range of their bits.
    pub(crate) full_range: bool,
    /// Where each chroma sample sits among the luma samples.
    pub(crate) site: Site,
}

/// The matrix coefficients this crate converts.
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

#[cfg(test)]
#[path = "../tests/unit/colour.rs"]
mod tests;
