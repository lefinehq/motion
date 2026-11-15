//! The sound of a video ([D56]): the mix, encoded for its container. An MP4
//! file takes AAC-LC from the system's encoder, where it has one ([D13]); a
//! QuickTime file takes 16-bit PCM, dithered down from the mix's floats.
//!
//! [D13]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d13-codec-licence-policy
//! [D56]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d56-audio-layers-mixed-at-48-khz-and-read-by-effects

use motion_format::{Diagnostic, Kind, Result};

use crate::video::Container;

/// AAC's bit rate for a stereo track: transparent for music.
#[cfg(target_os = "macos")]
const AAC_BITRATE: u32 = 192_000;
/// Frames in each AAC packet.
pub const AAC_FRAMES: u32 = 1024;

/// A video's sound: one buffer of float samples a channel, from -1 to 1,
/// all of one length.
#[derive(Debug, Clone, PartialEq)]
pub struct Soundtrack {
    /// Frames a second.
    pub rate: u32,
    /// The samples of each channel, left then right for stereo.
    pub channels: Vec<Vec<f32>>,
}

impl Soundtrack {
    /// How many frames it holds.
    pub fn frames(&self) -> u64 {
        self.channels.first().map_or(0, |c| c.len() as u64)
    }

    /// The samples, frame by frame.
    fn interleaved(&self) -> Vec<f32> {
        let frames = self.frames() as usize;
        (0..frames)
            .flat_map(|i| self.channels.iter().map(move |c| c[i]))
            .collect()
    }
}

/// A soundtrack encoded for a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodedSound {
    /// AAC-LC: packets of [`AAC_FRAMES`] frames each, after `leading`
    /// frames of the encoder's priming.
    Aac {
        /// The packets, in order.
        packets: Vec<Vec<u8>>,
        /// Frames of priming before the first frame of sound.
        leading: u32,
        /// Frames of sound, without the priming.
        frames: u64,
        /// Frames a second.
        rate: u32,
        /// Channels.
        channels: u16,
        /// Bits a second.
        bitrate: u32,
    },
    /// 16-bit little-endian PCM, frame by frame.
    Pcm16 {
        /// The samples.
        data: Vec<u8>,
        /// Frames of sound.
        frames: u64,
        /// Frames a second.
        rate: u32,
        /// Channels.
        channels: u16,
    },
}

impl EncodedSound {
    /// Its frames a second.
    pub fn rate(&self) -> u32 {
        match self {
            EncodedSound::Aac { rate, .. } | EncodedSound::Pcm16 { rate, .. } => *rate,
        }
    }

    /// How many frames of sound it plays.
    pub fn frames(&self) -> u64 {
        match self {
            EncodedSound::Aac { frames, .. } | EncodedSound::Pcm16 { frames, .. } => *frames,
        }
    }
}

/// `track` encoded as `container` holds sound: AAC for MP4, PCM for MOV.
///
/// # Errors
/// A diagnostic for a track that is not mono or stereo, has channels of
/// different lengths or a rate AAC does not take, or for an MP4 file where
/// the system has no AAC encoder.
pub fn encode_sound(track: &Soundtrack, container: Container) -> Result<EncodedSound> {
    let channels = match track.channels.len() {
        n @ (1 | 2) => n as u16,
        n => {
            return Err(
                Diagnostic::new(Kind::Invalid, format!("a sound of {n} channels"))
                    .expected("mono or stereo"),
            );
        }
    };
    if track
        .channels
        .iter()
        .any(|c| c.len() as u64 != track.frames())
    {
        return Err(
            Diagnostic::new(Kind::Invalid, "sound channels of different lengths")
                .expected("one length for every channel"),
        );
    }
    match container {
        Container::Mov => Ok(EncodedSound::Pcm16 {
            data: pcm16(&track.interleaved()),
            frames: track.frames(),
            rate: track.rate,
            channels,
        }),
        Container::Mp4 => aac(track, channels),
    }
}

#[cfg(target_os = "macos")]
fn aac(track: &Soundtrack, channels: u16) -> Result<EncodedSound> {
    if frequency_index(track.rate).is_none() {
        return Err(Diagnostic::new(
            Kind::OutOfRange,
            format!("sound at {} Hz in an MP4 file", track.rate),
        )
        .expected("a rate AAC takes, such as 48000 Hz"));
    }
    let bitrate = AAC_BITRATE / 2 * u32::from(channels);
    let encoded = motion_videotoolbox::encode_aac(
        &track.interleaved(),
        f64::from(track.rate),
        u32::from(channels),
        bitrate,
    )
    .map_err(|e| Diagnostic::new(Kind::Invalid, format!("the system's AAC encoder: {e}")))?;
    Ok(EncodedSound::Aac {
        packets: encoded.packets,
        leading: encoded.leading,
        frames: track.frames(),
        rate: track.rate,
        channels,
        bitrate,
    })
}

#[cfg(not(target_os = "macos"))]
fn aac(_track: &Soundtrack, _channels: u16) -> Result<EncodedSound> {
    Err(Diagnostic::new(
        Kind::Invalid,
        "sound in an MP4 file: this system has no AAC encoder motion can use",
    )
    .expected("a `.mov` file, which keeps the sound as PCM, or a `.wav` file for the sound alone"))
}

/// The index of `rate` in MPEG-4 audio's table of sampling frequencies.
pub(crate) fn frequency_index(rate: u32) -> Option<u8> {
    const RATES: [u32; 13] = [
        96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
        8_000, 7_350,
    ];
    RATES.iter().position(|&r| r == rate).map(|i| i as u8)
}

/// `samples` as 16-bit little-endian PCM, with triangular dither of one
/// step so quiet fades do not turn into steps. The noise comes from a
/// fixed seed, so the same mix gives the same bytes.
pub(crate) fn pcm16(samples: &[f32]) -> Vec<u8> {
    let mut state: u32 = 0x9E37_79B9;
    let mut uniform = move || {
        // xorshift32: fast, and the same on every machine.
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        f64::from(state) / f64::from(u32::MAX)
    };
    samples
        .iter()
        .flat_map(|&s| {
            let noise = uniform() - uniform();
            let level = (f64::from(s) * 32_767.0 + noise).round();
            (level.clamp(-32_768.0, 32_767.0) as i16).to_le_bytes()
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/sound.rs"]
mod tests;
