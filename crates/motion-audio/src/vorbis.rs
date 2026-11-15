//! Ogg Vorbis, through `lewton`, with the speakers in Vorbis's own order,
//! trimmed to the length the last page gives.

use std::io::Cursor;

use motion_format::Result;

use crate::{Sound, Speaker, broken};

/// Whether `bytes` start an Ogg stream whose first packet is a Vorbis
/// identification header.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    // The first page's header is 27 bytes, then one lacing value per
    // segment; the packet starts after them.
    let Some(&segments) = bytes.get(26) else {
        return false;
    };
    let packet = 27 + usize::from(segments);
    bytes.starts_with(b"OggS") && bytes.get(packet..packet + 7) == Some(b"\x01vorbis")
}

/// Decodes a whole Ogg Vorbis stream.
pub(crate) fn decode(bytes: &[u8]) -> Result<Sound> {
    let mut reader =
        lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(broken)?;
    let count = usize::from(reader.ident_hdr.audio_channels);
    let rate = reader.ident_hdr.audio_sample_rate;
    let mut channels: Vec<Vec<f32>> = vec![Vec::new(); count];
    while let Some(packet) = reader
        .read_dec_packet_generic::<Vec<Vec<f32>>>()
        .map_err(broken)?
    {
        for (channel, samples) in channels.iter_mut().zip(packet) {
            channel.extend(samples);
        }
    }
    // The last page's granule position is the stream's length in samples:
    // the last packet decodes past it, to a whole block.
    if let Some(length) = reader
        .get_last_absgp()
        .and_then(|n| usize::try_from(n).ok())
    {
        for channel in &mut channels {
            channel.truncate(length);
        }
    }
    Sound::new(rate, channels, layout(count))
}

/// Where Vorbis puts `count` channels (Vorbis I specification, 4.3.9).
fn layout(count: usize) -> Vec<Speaker> {
    use Speaker::{
        BackCenter, BackLeft, BackRight, FrontCenter, FrontLeft, FrontRight, LowFrequency,
        SideLeft, SideRight,
    };
    match count {
        3 => vec![FrontLeft, FrontCenter, FrontRight],
        5 => vec![FrontLeft, FrontCenter, FrontRight, BackLeft, BackRight],
        6 => vec![
            FrontLeft,
            FrontCenter,
            FrontRight,
            BackLeft,
            BackRight,
            LowFrequency,
        ],
        7 => vec![
            FrontLeft,
            FrontCenter,
            FrontRight,
            SideLeft,
            SideRight,
            BackCenter,
            LowFrequency,
        ],
        8 => vec![
            FrontLeft,
            FrontCenter,
            FrontRight,
            SideLeft,
            SideRight,
            BackLeft,
            BackRight,
            LowFrequency,
        ],
        n => Speaker::default_layout(n),
    }
}
