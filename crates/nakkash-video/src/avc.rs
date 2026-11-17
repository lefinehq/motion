//! The H.264 decoder configuration record (`avcC`), what its sequence
//! parameter set says about turning decoded samples into colour (the video
//! signal type, with range and colour description, and the chroma siting),
//! and which samples no other refers to. Decoding itself is a backend's; this reads only what the
//! conversion after it needs, on every platform.

use nakkash_format::{Diagnostic, Kind, Result};

/// What `avcC` says about a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Record {
    /// Bytes of the length before each NAL unit in a sample: 1, 2 or 4.
    pub(crate) length_size: usize,
    /// Its first sequence parameter set.
    pub(crate) sps: Sps,
}

/// What a sequence parameter set says about the pictures' colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sps {
    /// The video signal type, when the stream carries one.
    pub(crate) signal: Option<Signal>,
    /// Where chroma samples sit in a frame, 0 to 5 as H.273 numbers it
    /// (`chroma_sample_loc_type_top_field`), when the stream says.
    pub(crate) chroma_site: Option<u32>,
}

/// A video signal type: its range, and its colour description when present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Signal {
    /// Whether samples use the full range of their bits.
    pub(crate) full_range: bool,
    /// Colour primaries, transfer characteristics and matrix coefficients,
    /// as H.273 numbers them.
    pub(crate) description: Option<[u8; 3]>,
}

impl Record {
    /// Reads `avcC`'s body.
    ///
    /// # Errors
    /// An invalid-value diagnostic when the record is cut short, of a
    /// version other than 1, or holds no sequence parameter set, as an
    /// `avc3` stream that carries them only in its samples may.
    pub(crate) fn parse(record: &[u8]) -> Result<Record> {
        let byte = |at: usize| {
            record
                .get(at)
                .copied()
                .ok_or_else(|| invalid("a cut-short `avcC`"))
        };
        if byte(0)? != 1 {
            return Err(invalid(format!("`avcC` version {}", byte(0)?)));
        }
        let length_size = (byte(4)? & 3) + 1;
        if length_size == 3 {
            return Err(invalid("NAL unit lengths of 3 bytes"));
        }
        let count = byte(5)? & 31;
        if count == 0 {
            return Err(invalid(
                "no sequence parameter set in `avcC` (one carried only in the samples)",
            ));
        }
        let length = usize::from(u16::from_be_bytes([byte(6)?, byte(7)?]));
        let sps = record
            .get(8..8 + length)
            .ok_or_else(|| invalid("a parameter set past the end of `avcC`"))?;
        Ok(Record {
            length_size: usize::from(length_size),
            sps: Sps::parse(sps)?,
        })
    }
}

/// Whether no other picture refers to the one in `sample`, NAL units each
/// after a `length_size`-byte length: it has slices, and every one has a
/// `nal_ref_idc` of 0. Such a sample can go undecoded when its own frame
/// is not wanted. A sample cut short is not one.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn disposable(sample: &[u8], length_size: usize) -> bool {
    let mut rest = sample;
    let mut slices = 0;
    while rest.len() > length_size {
        let length = rest[..length_size]
            .iter()
            .fold(0usize, |n, &b| n << 8 | usize::from(b));
        let Some(nal) = rest.get(length_size..length_size + length) else {
            return false;
        };
        if let Some(&header) = nal.first()
            && matches!(header & 31, 1..=5)
        {
            if header & 0x60 != 0 {
                return false;
            }
            slices += 1;
        }
        rest = &rest[length_size + length..];
    }
    slices > 0
}

impl Sps {
    /// Reads a sequence parameter set NAL unit, its header byte first.
    ///
    /// # Errors
    /// An invalid-value diagnostic when it is not one, or is cut short
    /// before the parts read.
    pub(crate) fn parse(nal: &[u8]) -> Result<Sps> {
        if nal.first().map(|h| h & 31) != Some(7) {
            return Err(invalid("a sequence parameter set that is not one"));
        }
        let rbsp = unescape(&nal[1..]);
        let mut bits = Bits::new(&rbsp);
        let cut = || invalid("a cut-short sequence parameter set");
        let profile = bits.read(8).ok_or_else(cut)?;
        bits.read(16).ok_or_else(cut)?; // constraint flags and level
        bits.golomb().ok_or_else(cut)?; // seq_parameter_set_id
        if matches!(
            profile,
            100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
        ) {
            let chroma_format = bits.golomb().ok_or_else(cut)?;
            if chroma_format == 3 {
                bits.read(1).ok_or_else(cut)?; // separate_colour_plane_flag
            }
            bits.golomb().ok_or_else(cut)?; // bit_depth_luma_minus8
            bits.golomb().ok_or_else(cut)?; // bit_depth_chroma_minus8
            bits.read(1).ok_or_else(cut)?; // qpprime_y_zero_transform_bypass_flag
            if bits.read(1).ok_or_else(cut)? == 1 {
                let lists = if chroma_format == 3 { 12 } else { 8 };
                for i in 0..lists {
                    if bits.read(1).ok_or_else(cut)? == 1 {
                        skip_scaling_list(&mut bits, if i < 6 { 16 } else { 64 })
                            .ok_or_else(cut)?;
                    }
                }
            }
        }
        bits.golomb().ok_or_else(cut)?; // log2_max_frame_num_minus4
        match bits.golomb().ok_or_else(cut)? {
            0 => {
                bits.golomb().ok_or_else(cut)?; // log2_max_pic_order_cnt_lsb_minus4
            }
            1 => {
                bits.read(1).ok_or_else(cut)?; // delta_pic_order_always_zero_flag
                bits.signed_golomb().ok_or_else(cut)?; // offset_for_non_ref_pic
                bits.signed_golomb().ok_or_else(cut)?; // offset_for_top_to_bottom_field
                for _ in 0..bits.golomb().ok_or_else(cut)? {
                    bits.signed_golomb().ok_or_else(cut)?; // offset_for_ref_frame
                }
            }
            _ => {}
        }
        bits.golomb().ok_or_else(cut)?; // max_num_ref_frames
        bits.read(1).ok_or_else(cut)?; // gaps_in_frame_num_value_allowed_flag
        bits.golomb().ok_or_else(cut)?; // pic_width_in_mbs_minus1
        bits.golomb().ok_or_else(cut)?; // pic_height_in_map_units_minus1
        if bits.read(1).ok_or_else(cut)? == 0 {
            bits.read(1).ok_or_else(cut)?; // mb_adaptive_frame_field_flag
        }
        bits.read(1).ok_or_else(cut)?; // direct_8x8_inference_flag
        if bits.read(1).ok_or_else(cut)? == 1 {
            for _ in 0..4 {
                bits.golomb().ok_or_else(cut)?; // frame cropping offsets
            }
        }
        let mut sps = Sps {
            signal: None,
            chroma_site: None,
        };
        // A stream cut short inside its VUI keeps what was read before.
        if bits.read(1) == Some(1) {
            vui(&mut bits, &mut sps);
        }
        Ok(sps)
    }
}

/// The start of the VUI parameters, up to the chroma siting; what is not
/// there is left unset.
fn vui(bits: &mut Bits<'_>, sps: &mut Sps) -> Option<()> {
    if bits.read(1)? == 1 && bits.read(8)? == 255 {
        bits.read(32)?; // sar_width and sar_height
    }
    if bits.read(1)? == 1 {
        bits.read(1)?; // overscan_appropriate_flag
    }
    if bits.read(1)? == 1 {
        bits.read(3)?; // video_format
        let full_range = bits.read(1)? == 1;
        let description = match bits.read(1)? {
            1 => Some([
                bits.read(8)? as u8,
                bits.read(8)? as u8,
                bits.read(8)? as u8,
            ]),
            _ => None,
        };
        sps.signal = Some(Signal {
            full_range,
            description,
        });
    }
    if bits.read(1)? == 1 {
        sps.chroma_site = Some(bits.golomb()?);
    }
    Some(())
}

/// Reads past one `scaling_list` of `size` coefficients.
fn skip_scaling_list(bits: &mut Bits<'_>, size: usize) -> Option<()> {
    let (mut last, mut next) = (8i64, 8i64);
    for _ in 0..size {
        if next != 0 {
            next = (last + i64::from(bits.signed_golomb()?)).rem_euclid(256);
        }
        if next != 0 {
            last = next;
        }
    }
    Some(())
}

/// A NAL unit's payload without its emulation prevention bytes: each `03`
/// after two zero bytes is dropped.
fn unescape(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len());
    let mut zeros = 0;
    for &byte in payload {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

/// A reader of bits, most significant first.
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Bits<'a> {
    fn new(bytes: &'a [u8]) -> Bits<'a> {
        Bits { bytes, at: 0 }
    }

    /// The next `count` bits, up to 32, as a number.
    fn read(&mut self, count: usize) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            let byte = self.bytes.get(self.at / 8)?;
            value = (value << 1) | u32::from((byte >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Some(value)
    }

    /// An unsigned Exp-Golomb number.
    fn golomb(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.read(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let rest = self.read(zeros)?;
        Some(((1u64 << zeros) - 1 + u64::from(rest)).min(u64::from(u32::MAX)) as u32)
    }

    /// A signed Exp-Golomb number.
    fn signed_golomb(&mut self) -> Option<i32> {
        let k = i64::from(self.golomb()?);
        let value = if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) };
        Some(value as i32)
    }
}

fn invalid(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("H.264: {}", message.into()))
        .expected("an H.264 stream with its parameter sets in `avcC`")
}

#[cfg(test)]
#[path = "../tests/unit/avc.rs"]
mod tests;
