//! MP3, and MPEG layers I and II, through `nanomp3`, a pure Rust port of
//! minimp3: tags skipped, the encoder's delay and padding trimmed when the
//! stream's tag says them, so a loop cut in the encoder stays gapless.

use motion_format::Result;

use crate::{Sound, broken};

/// Whether `bytes` start an MPEG audio stream: an ID3 tag, or a frame
/// whose next frame follows where its header says.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3") || nanomp3::detect(bytes)
}

/// Decodes a whole MPEG audio stream.
pub(crate) fn decode(bytes: &[u8]) -> Result<Sound> {
    let decoded = nanomp3::decode_all::<f32>(bytes);
    let channels = decoded
        .channels
        .ok_or_else(|| broken("no audio frame, or a stream that switches channels"))?;
    if decoded.format_changed {
        return Err(broken(
            "a stream whose rate or layer changes partway, which one sound cannot hold",
        ));
    }
    Sound::interleaved(
        decoded.sample_rate,
        usize::from(channels.num()),
        &decoded.samples,
    )
}
