//! WAV files: RIFF, and RF64 or BW64 for files past 4 GiB. Integer PCM of
//! 8 to 32 bits, IEEE float of 32 and 64, A-law and µ-law, in plain or
//! extensible format chunks; the extensible chunk's channel mask says where
//! each channel stands. A data chunk whose size runs past the file, as a
//! recorder that never finished writes, is read to the end.

use motion_format::Result;

use crate::{Sound, Speaker, broken, scaled};

/// The format codes this reader decodes.
const PCM: u16 = 1;
const FLOAT: u16 = 3;
const A_LAW: u16 = 6;
const MU_LAW: u16 = 7;
const EXTENSIBLE: u16 = 0xFFFE;

/// Whether `bytes` start a WAV file.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some(b"RIFF" | b"RF64" | b"BW64")) && bytes.get(8..12) == Some(b"WAVE")
}

/// What the format chunk says.
#[derive(Debug, Clone, Copy)]
struct Shape {
    code: u16,
    channels: usize,
    rate: u32,
    /// Bytes a sample of one channel takes.
    width: usize,
    mask: u32,
}

/// Decodes a whole WAV file.
pub(crate) fn decode(bytes: &[u8]) -> Result<Sound> {
    let mut shape = None;
    let mut data = None;
    // RF64 and BW64 keep the data chunk's real size in `ds64`.
    let mut long_data = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4) as usize;
        let body = at + 8;
        match id {
            b"ds64" if size >= 16 => long_data = Some(u64_at(bytes, body + 8)),
            b"fmt " => shape = Some(format(bytes.get(body..body + size).ok_or_else(cut)?)?),
            b"data" => {
                let size = match (size as u32, long_data) {
                    (u32::MAX, Some(long)) => usize::try_from(long).unwrap_or(usize::MAX),
                    _ => size,
                };
                // A size past the end: read what is there.
                let end = body.saturating_add(size).min(bytes.len());
                data = Some(&bytes[body..end]);
                break;
            }
            _ => {}
        }
        // Chunks are padded to an even length.
        at = body.saturating_add(size).saturating_add(size & 1);
    }
    let shape = shape.ok_or_else(|| broken("no `fmt ` chunk"))?;
    let data = data.ok_or_else(|| broken("no `data` chunk"))?;
    samples(shape, data)
}

/// The format chunk's `body`.
fn format(body: &[u8]) -> Result<Shape> {
    if body.len() < 16 {
        return Err(broken("a `fmt ` chunk shorter than 16 bytes"));
    }
    let mut code = u16_at(body, 0);
    let channels = usize::from(u16_at(body, 2));
    let rate = u32_at(body, 4);
    let align = usize::from(u16_at(body, 12));
    let bits = usize::from(u16_at(body, 14));
    let mut mask = 0;
    if code == EXTENSIBLE {
        if body.len() < 40 {
            return Err(broken("an extensible `fmt ` chunk shorter than 40 bytes"));
        }
        mask = u32_at(body, 20);
        // The sub-format GUID starts with the format code.
        code = u16_at(body, 24);
    }
    if channels == 0 || align == 0 || align % channels != 0 {
        return Err(broken(format!(
            "{channels} channels in blocks of {align} bytes"
        )));
    }
    let width = align / channels;
    let fits = match code {
        PCM => (1..=4).contains(&width) && bits <= width * 8,
        FLOAT => width == 4 || width == 8,
        A_LAW | MU_LAW => width == 1,
        _ => {
            return Err(broken(format!(
                "format code {code:#06x}, which motion cannot decode yet"
            )));
        }
    };
    if !fits {
        return Err(broken(format!(
            "{bits}-bit samples in {width} bytes for format code {code}"
        )));
    }
    Ok(Shape {
        code,
        channels,
        rate,
        width,
        mask,
    })
}

/// The sound in `data`, interleaved as `shape` says.
fn samples(shape: Shape, data: &[u8]) -> Result<Sound> {
    let Shape {
        code,
        channels,
        width,
        ..
    } = shape;
    let frames = data.len() / (channels * width);
    let mut out = vec![Vec::with_capacity(frames); channels];
    for (i, sample) in data.chunks_exact(width).take(frames * channels).enumerate() {
        let value = match (code, width) {
            // 8-bit PCM is unsigned, centred on 128.
            (PCM, 1) => (f32::from(sample[0]) - 128.0) / 128.0,
            (PCM, _) => {
                // Little-endian, sign-extended from the top byte, scaled
                // by the container: smaller valid bits sit at the top.
                let mut word = [0_u8; 4];
                word[4 - width..].copy_from_slice(sample);
                scaled(
                    i32::from_le_bytes(word) >> (8 * (4 - width)),
                    8 * width as u32,
                )
            }
            (FLOAT, 4) => f32::from_le_bytes(sample.try_into().unwrap_or_default()),
            (FLOAT, _) => f64::from_le_bytes(sample.try_into().unwrap_or_default()) as f32,
            (A_LAW, _) => a_law(sample[0]),
            _ => mu_law(sample[0]),
        };
        out[i % channels].push(value);
    }
    Sound::new(shape.rate, out, Speaker::from_mask(shape.mask, channels))
}

/// An A-law byte as a sample (ITU-T G.711).
fn a_law(byte: u8) -> f32 {
    let byte = byte ^ 0x55;
    let exponent = (byte >> 4) & 0x07;
    let mantissa = i32::from(byte & 0x0F);
    let magnitude = match exponent {
        0 => (mantissa << 4) + 8,
        e => ((mantissa << 4) + 0x108) << (e - 1),
    };
    let value = if byte & 0x80 != 0 {
        magnitude
    } else {
        -magnitude
    };
    value as f32 / 32768.0
}

/// A µ-law byte as a sample (ITU-T G.711).
fn mu_law(byte: u8) -> f32 {
    let byte = !byte;
    let exponent = (byte >> 4) & 0x07;
    let mantissa = i32::from(byte & 0x0F);
    let magnitude = (((mantissa << 3) + 0x84) << exponent) - 0x84;
    let value = if byte & 0x80 != 0 {
        -magnitude
    } else {
        magnitude
    };
    value as f32 / 32768.0
}

/// Why a chunk is cut short.
fn cut() -> motion_format::Diagnostic {
    broken("a chunk cut short by the end of the file")
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    bytes
        .get(at..at + 2)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    bytes
        .get(at..at + 8)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u64::from_le_bytes)
}

/// A WAV file of `sound`'s samples as 32-bit float, for writing a mix to
/// disk. Mono and stereo are plain float WAV, which every reader takes;
/// more channels, or speakers other than the usual, add the extensible
/// chunk's channel mask to say where they stand.
pub fn encode(sound: &Sound) -> Vec<u8> {
    let channels = sound.channels().len();
    let frames = sound.frames();
    let data = frames * channels * 4;
    let plain = sound.speakers() == Speaker::default_layout(channels) && channels <= 2;
    let mut format = Vec::with_capacity(40);
    format.extend_from_slice(&if plain { FLOAT } else { EXTENSIBLE }.to_le_bytes());
    format.extend_from_slice(&(channels as u16).to_le_bytes());
    format.extend_from_slice(&sound.rate().to_le_bytes());
    format.extend_from_slice(&(sound.rate() * 4 * channels as u32).to_le_bytes());
    format.extend_from_slice(&(4 * channels as u16).to_le_bytes());
    format.extend_from_slice(&32_u16.to_le_bytes());
    if plain {
        format.extend_from_slice(&0_u16.to_le_bytes());
    } else {
        let mask: u32 = sound
            .speakers()
            .iter()
            .filter_map(|s| s.mask_bit())
            .fold(0, |m, bit| m | (1 << bit));
        format.extend_from_slice(&22_u16.to_le_bytes());
        format.extend_from_slice(&32_u16.to_le_bytes());
        format.extend_from_slice(&mask.to_le_bytes());
        // KSDATAFORMAT_SUBTYPE_IEEE_FLOAT.
        format.extend_from_slice(&[
            0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38,
            0x9B, 0x71,
        ]);
    }
    let mut out = Vec::with_capacity(48 + format.len() + data);
    out.extend_from_slice(b"RIFF");
    let riff = 4 + 8 + format.len() + 12 + 8 + data;
    out.extend_from_slice(&u32::try_from(riff).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&(format.len() as u32).to_le_bytes());
    out.extend_from_slice(&format);
    // A format other than integer PCM says its length in a `fact` chunk.
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4_u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(frames).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&u32::try_from(data).unwrap_or(u32::MAX).to_le_bytes());
    for f in 0..frames {
        for channel in sound.channels() {
            out.extend_from_slice(&channel[f].to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/wav.rs"]
mod tests;
