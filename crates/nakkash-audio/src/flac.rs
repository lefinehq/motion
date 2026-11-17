//! FLAC, through `claxon`: integer samples of 4 to 32 bits, scaled by the
//! stream's own depth.

use std::io::Cursor;

use nakkash_format::Result;

use crate::{Sound, broken, scaled};

/// Whether `bytes` start a FLAC stream.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"fLaC")
}

/// Decodes a whole FLAC stream.
pub(crate) fn decode(bytes: &[u8]) -> Result<Sound> {
    let mut reader = claxon::FlacReader::new(Cursor::new(bytes)).map_err(broken)?;
    let info = reader.streaminfo();
    let count = info.channels as usize;
    let bits = info.bits_per_sample;
    let mut interleaved = Vec::with_capacity(
        info.samples
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0)
            .saturating_mul(count),
    );
    for sample in reader.samples() {
        interleaved.push(scaled(sample.map_err(broken)?, bits));
    }
    Sound::interleaved(info.sample_rate, count, &interleaved)
}
