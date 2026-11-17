//! H.264 (`avc1`, `avc3`), decoded by the operating system's decoder where
//! there is one ([D60]): VideoToolbox on macOS, in `toolbox`. The colour of
//! the decoded samples comes from the clip, read here on every platform, so
//! a backend only hands back planes; one loaded at run time for the other
//! platforms slots in beside the system's in `backend`.
//!
//! [D60]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d60-h264-footage-decodes-through-the-systems-decoder

use nakkash_format::{Diagnostic, Kind, Result};

use crate::Decoder;
use crate::avc::Record;
use crate::colour::Colour;
use crate::container::Track;

/// Whether H.264 decodes on this platform.
pub(crate) const HERE: bool = cfg!(target_os = "macos");

/// A decoder for the H.264 `track`.
///
/// # Errors
/// An invalid-value diagnostic when the track has no usable `avcC`, its
/// colour cannot be converted, or there is no decoder for it here; the
/// system's own failure to set up a decoder for it.
pub(crate) fn decoder(track: &Track) -> Result<Box<dyn Decoder>> {
    let record = track
        .extensions
        .iter()
        .find(|(kind, _)| kind == b"avcC")
        .map(|(_, body)| body.as_slice())
        .ok_or_else(|| {
            Diagnostic::new(Kind::Invalid, "H.264 without its configuration (`avcC`)")
                .expected("an H.264 stream with its parameter sets in `avcC`")
        })?;
    let parsed = Record::parse(record)?;
    let colour = Colour::of(track, parsed.sps.signal, parsed.sps.chroma_site)?;
    backend(track, record, parsed.length_size, colour)
}

#[cfg(target_os = "macos")]
fn backend(
    track: &Track,
    record: &[u8],
    length_size: usize,
    colour: Colour,
) -> Result<Box<dyn Decoder>> {
    crate::toolbox::Avc::new(record, length_size, track.width, track.height, colour)
        .map(|d| Box::new(d) as Box<dyn Decoder>)
}

#[cfg(not(target_os = "macos"))]
fn backend(
    track: &Track,
    _record: &[u8],
    _length_size: usize,
    _colour: Colour,
) -> Result<Box<dyn Decoder>> {
    Err(unsupported_here(&crate::codec_name(track.codec)))
}

/// Why a clip in `codec`, which has a decoder on another platform, does not
/// decode on this one.
pub(crate) fn unsupported_here(codec: &str) -> Diagnostic {
    Diagnostic::new(
        Kind::Invalid,
        format!(
            "video in {codec}, which nakkash decodes only through the operating \
             system's decoder, and it has none for this platform yet"
        ),
    )
    .expected("macOS, whose VideoToolbox decodes it")
}
