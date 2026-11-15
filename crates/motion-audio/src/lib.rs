//! Sound files read without system libraries, and laid out for mixing.
//!
//! [`open`] reads a file whatever its name says, by its first bytes: WAV
//! ([`wav`], our own reader), MP3, FLAC and Ogg Vorbis (pure Rust decoders,
//! each behind one `Format` row). A file comes out as a [`Sound`]: its own
//! rate, one buffer of samples from -1 to 1 per channel, and where each
//! channel's speaker stands. [`Sound::for_mix`] folds it down to stereo
//! ([`Speaker`]) and resamples it to [`MIX_RATE`] with a band-limited
//! kernel ([`Kernel`]), which also reads a sound at any position for
//! playing it slower or faster.
//!
//! This crate knows nothing about scenes, as `motion-video` does not.

mod flac;
mod kernel;
mod mp3;
mod speaker;
mod vorbis;
pub mod wav;

use std::path::Path;

use motion_format::{Diagnostic, Kind, Result};

pub use kernel::Kernel;
pub use speaker::Speaker;

/// The rate every sound is mixed at, in samples a second.
pub const MIX_RATE: u32 = 48_000;

/// A sound: its rate, and its samples, one buffer per channel, each sample
/// from -1 to 1 at full scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    rate: u32,
    channels: Vec<Vec<f32>>,
    speakers: Vec<Speaker>,
}

impl Sound {
    /// A sound of `channels` at `rate`, whose speakers are `speakers`, one a
    /// channel.
    ///
    /// # Errors
    /// An invalid-value diagnostic when the rate is 0, there is no channel,
    /// the channels differ in length, or the speakers are not one a channel.
    pub fn new(rate: u32, channels: Vec<Vec<f32>>, speakers: Vec<Speaker>) -> Result<Sound> {
        let invalid = |what: String| Err(Diagnostic::new(Kind::Invalid, what));
        if rate == 0 {
            return invalid("a sound at 0 samples a second".into());
        }
        let Some(first) = channels.first() else {
            return invalid("a sound with no channel".into());
        };
        if channels.iter().any(|c| c.len() != first.len()) {
            return invalid("a sound whose channels differ in length".into());
        }
        if speakers.len() != channels.len() {
            return invalid(format!(
                "{} speakers for {} channels",
                speakers.len(),
                channels.len()
            ));
        }
        Ok(Sound {
            rate,
            channels,
            speakers,
        })
    }

    /// A sound from `interleaved` samples of `count` channels, standing
    /// where `count` channels stand by default (see [`Speaker::default_layout`]).
    ///
    /// # Errors
    /// As [`Sound::new`]; also when `count` is 0.
    pub fn interleaved(rate: u32, count: usize, interleaved: &[f32]) -> Result<Sound> {
        if count == 0 {
            return Err(Diagnostic::new(Kind::Invalid, "a sound with no channel"));
        }
        let frames = interleaved.len() / count;
        let channels = (0..count)
            .map(|c| (0..frames).map(|f| interleaved[f * count + c]).collect())
            .collect();
        Sound::new(rate, channels, Speaker::default_layout(count))
    }

    /// Samples a second.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// One buffer of samples per channel.
    pub fn channels(&self) -> &[Vec<f32>] {
        &self.channels
    }

    /// Where each channel's speaker stands.
    pub fn speakers(&self) -> &[Speaker] {
        &self.speakers
    }

    /// How many samples each channel holds.
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    /// How long it lasts, in seconds.
    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate)
    }

    /// The same sound on two speakers, left and right: a mono channel
    /// plays on both, and more channels fold down by their speakers (see
    /// [`Speaker::fold`]).
    #[must_use]
    pub fn stereo(self) -> Sound {
        if self.speakers == [Speaker::FrontLeft, Speaker::FrontRight] {
            return self;
        }
        let frames = self.frames();
        let mut left = vec![0.0_f32; frames];
        let mut right = vec![0.0_f32; frames];
        let alone = self.channels.len() == 1;
        for (channel, speaker) in self.channels.iter().zip(&self.speakers) {
            let (l, r) = match alone {
                true => (1.0, 1.0),
                false => speaker.fold(),
            };
            for ((sample, left), right) in channel.iter().zip(&mut left).zip(&mut right) {
                *left += sample * l;
                *right += sample * r;
            }
        }
        Sound {
            rate: self.rate,
            channels: vec![left, right],
            speakers: vec![Speaker::FrontLeft, Speaker::FrontRight],
        }
    }

    /// The same sound at `rate`, through [`Kernel::standard`]. A sound
    /// already at `rate` is returned as it is.
    ///
    /// # Panics
    /// When `rate` is 0.
    #[must_use]
    pub fn resampled(self, rate: u32) -> Sound {
        assert!(rate > 0, "a rate of 0 samples a second");
        if rate == self.rate {
            return self;
        }
        let kernel = Kernel::standard();
        let from = self.rate;
        // Each channel on a thread of its own.
        let channels = std::thread::scope(|scope| {
            let running: Vec<_> = self
                .channels
                .iter()
                .map(|channel| scope.spawn(|| kernel.resample(channel, from, rate)))
                .collect();
            running
                .into_iter()
                .map(|thread| {
                    thread
                        .join()
                        .unwrap_or_else(|e| std::panic::resume_unwind(e))
                })
                .collect()
        });
        Sound {
            rate,
            channels,
            speakers: self.speakers,
        }
    }

    /// The sound as the mix takes it: stereo at [`MIX_RATE`].
    #[must_use]
    pub fn for_mix(self) -> Sound {
        self.stereo().resampled(MIX_RATE)
    }
}

/// A sound file format: its name, how to tell it by its first bytes, and
/// how to decode a whole file of it.
struct Format {
    name: &'static str,
    sniff: fn(&[u8]) -> bool,
    decode: fn(&[u8]) -> Result<Sound>,
}

/// Every format there is a decoder for, in the order they are tried.
const FORMATS: &[Format] = &[
    Format {
        name: "WAV",
        sniff: wav::sniff,
        decode: wav::decode,
    },
    Format {
        name: "FLAC",
        sniff: flac::sniff,
        decode: flac::decode,
    },
    Format {
        name: "Ogg Vorbis",
        sniff: vorbis::sniff,
        decode: vorbis::decode,
    },
    Format {
        name: "MP3",
        sniff: mp3::sniff,
        decode: mp3::decode,
    },
];

/// Formats a file may be in that there is no decoder for yet, by their
/// first bytes, so the error names them.
fn unsupported(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => Some("an MP4 container (AAC)"),
        [b'F', b'O', b'R', b'M', _, _, _, _, b'A', b'I', b'F', ..] => Some("AIFF"),
        [b'O', b'g', b'g', b'S', ..] => Some("an Ogg stream other than Vorbis"),
        _ => None,
    }
}

/// The name of the format of a file starting with `bytes`, if there is a
/// decoder for it.
pub fn format_name(bytes: &[u8]) -> Option<&'static str> {
    FORMATS.iter().find(|f| (f.sniff)(bytes)).map(|f| f.name)
}

/// Decodes a whole sound file held in `bytes`, by the format its first
/// bytes say.
///
/// # Errors
/// An invalid-value diagnostic when the bytes are in no format there is a
/// decoder for, naming it when it is known, or when they do not decode.
pub fn decode(bytes: &[u8]) -> Result<Sound> {
    let Some(format) = FORMATS.iter().find(|f| (f.sniff)(bytes)) else {
        let what = unsupported(bytes).unwrap_or("no sound format motion knows");
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("sound in {what}, which motion cannot decode yet"),
        )
        .expected(format!(
            "a sound in {}",
            FORMATS
                .iter()
                .map(|f| f.name)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    (format.decode)(bytes).map_err(|d| Diagnostic {
        message: format!("{}: {}", format.name, d.message),
        ..d
    })
}

/// Reads and decodes the sound file at `path`.
///
/// # Errors
/// An I/O diagnostic naming the file when it cannot be read, or
/// [`decode`]'s, prefixed with the file.
pub fn open(path: &Path) -> Result<Sound> {
    let bytes = std::fs::read(path).map_err(|e| Diagnostic::io("read", path, &e))?;
    decode(&bytes).map_err(|d| Diagnostic {
        message: format!("`{}`: {}", path.display(), d.message),
        ..d
    })
}

/// A signed integer sample of `bits` bits as a sample from -1 to 1.
pub(crate) fn scaled(sample: i32, bits: u32) -> f32 {
    (f64::from(sample) / f64::from(1_u32 << (bits - 1))) as f32
}

/// A decoder's failure as an invalid-value diagnostic saying `what`.
pub(crate) fn broken(what: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("{what}"))
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
