//! The pieces of a video file and the writer that joins them: a codec
//! behind [`VideoEncoder`], a container behind [`Muxer`].

use std::fs;
use std::path::{Path, PathBuf};

use motion_format::{Diagnostic, Kind, Result};

use crate::colour::{Ayuv16, Planes, to_ayuv16, to_planes};
use crate::sound::{EncodedSound, Soundtrack, encode_sound};
#[cfg(target_os = "macos")]
use motion_videotoolbox::Codec as System;

/// The size and rate of a video.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoSpec {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frames per second, above zero.
    pub fps: f64,
}

/// One compressed frame, in the codec's own bitstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// The bitstream.
    pub data: Vec<u8>,
    /// The number of the frame it shows, counting from 0.
    pub frame: u64,
    /// Whether it decodes without any frame before it.
    pub keyframe: bool,
}

/// One frame, converted for its codec ([`crate::colour`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// 8-bit 4:2:0, alpha drawn over black: H.264, HEVC and AV1.
    Yuv420(Planes),
    /// 16-bit 4:4:4 with straight alpha: ProRes 4444.
    Ayuv16(Ayuv16),
}

/// What a container stores so a player can set up the codec's decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecoderConfig {
    /// An AV1 codec configuration record, its sequence header included.
    Av1 {
        /// The record's bytes, as the `av1C` box holds them.
        record: Vec<u8>,
    },
    /// An H.264 decoder configuration record: profile, level and parameter
    /// sets.
    Avc {
        /// The record's bytes, as the `avcC` box holds them.
        record: Vec<u8>,
    },
    /// An HEVC decoder configuration record.
    Hevc {
        /// The record's bytes, as the `hvcC` box holds them.
        record: Vec<u8>,
    },
    /// ProRes 4444, whose frames carry all a decoder needs.
    ProRes,
}

/// A codec: planes in, in order, packets out. It is `Send` so frames drawn
/// on several threads can be handed to it in turn.
pub trait VideoEncoder: Send {
    /// Takes the next frame, converted as [`Codec::keeps_alpha`] says, and
    /// returns the packets ready so far, which may be none while the codec
    /// looks ahead.
    ///
    /// # Errors
    /// The codec's failure, as a diagnostic.
    fn encode(&mut self, frame: &Frame) -> Result<Vec<Packet>>;

    /// Ends the stream and returns every packet still held.
    ///
    /// # Errors
    /// The codec's failure, as a diagnostic.
    fn finish(&mut self) -> Result<Vec<Packet>>;

    /// The decoder's configuration, once [`VideoEncoder::finish`] has
    /// returned: a codec knows all of it only after its first keyframe.
    fn config(&self) -> DecoderConfig;
}

/// A container: packets in, in frame order, a file out.
pub trait Muxer: Send {
    /// Writes one packet.
    ///
    /// # Errors
    /// A diagnostic when the packet cannot be written.
    fn write(&mut self, packet: &Packet) -> Result<()>;

    /// Writes what the container needs after the last packet, `config`
    /// among it, and the sound when there is one, and closes the file.
    ///
    /// # Errors
    /// A diagnostic when the file cannot be finished.
    fn finish(self: Box<Self>, config: &DecoderConfig, sound: Option<&EncodedSound>) -> Result<()>;
}

/// The codecs this build has, by the name `--codec` takes.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::VariantNames,
)]
#[strum(serialize_all = "lowercase")]
pub enum Codec {
    /// H.264 from the system's encoder, on macOS; the default there.
    #[cfg(target_os = "macos")]
    #[default]
    H264,
    /// HEVC from the system's encoder, on macOS.
    #[cfg(target_os = "macos")]
    Hevc,
    /// ProRes 4444 with alpha from the system's encoder, on macOS, in a
    /// `.mov` file: for scenes with a clear background, to edit further.
    #[cfg(target_os = "macos")]
    #[strum(serialize = "prores")]
    ProRes,
    /// AV1, through `rav1e`, on every platform; the default where the
    /// platform's own encoder has not landed.
    #[cfg_attr(not(target_os = "macos"), default)]
    Av1,
}

impl Codec {
    /// The codec's name as people write it: `H.264`, `HEVC`, `AV1`.
    pub fn label(self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Codec::H264 => "H.264",
            #[cfg(target_os = "macos")]
            Codec::Hevc => "HEVC",
            #[cfg(target_os = "macos")]
            Codec::ProRes => "ProRes 4444",
            Codec::Av1 => "AV1",
        }
    }

    /// Whether the codec keeps alpha: the others get frames drawn over
    /// black.
    pub fn keeps_alpha(self) -> bool {
        #[cfg(target_os = "macos")]
        if self == Codec::ProRes {
            return true;
        }
        false
    }

    /// The qualities the codec offers: ProRes has one, `master`.
    pub fn qualities(self) -> &'static [VideoQuality] {
        #[cfg(target_os = "macos")]
        if self == Codec::ProRes {
            return &[VideoQuality::Master];
        }
        &[VideoQuality::Draft, VideoQuality::Web, VideoQuality::Master]
    }

    /// The quality used when none is asked for: `web`, or the only one.
    pub fn default_quality(self) -> VideoQuality {
        match self.qualities() {
            [only] => *only,
            _ => VideoQuality::Web,
        }
    }

    /// The codec named `name`.
    ///
    /// # Errors
    /// An unknown-name diagnostic listing the codecs this build has.
    pub fn named(name: &str) -> Result<Codec> {
        name.parse().map_err(|_| {
            Diagnostic::unknown("codec", name, <Codec as strum::VariantNames>::VARIANTS)
        })
    }
}

/// How much a video keeps of the drawn frames, against its size and the
/// time it takes: a use, not a bitrate. Each codec has its own settings for
/// each.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::VariantNames,
)]
#[strum(serialize_all = "lowercase")]
pub enum VideoQuality {
    /// Fast and small, for checking timing and layout.
    Draft,
    /// For delivery: text and edges stay crisp.
    #[default]
    Web,
    /// Close to the drawn frames, for editing or encoding again.
    Master,
}

impl VideoQuality {
    /// The quality named `name`.
    ///
    /// # Errors
    /// An unknown-name diagnostic listing the qualities.
    pub fn named(name: &str) -> Result<VideoQuality> {
        name.parse().map_err(|_| {
            Diagnostic::unknown(
                "video quality",
                name,
                <VideoQuality as strum::VariantNames>::VARIANTS,
            )
        })
    }
}

/// The containers, by file extension.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::Display, strum::EnumString, strum::VariantNames,
)]
#[strum(serialize_all = "lowercase")]
pub enum Container {
    /// MP4, written by this crate on `mp4-atom`'s boxes.
    Mp4,
    /// QuickTime, the same boxes with QuickTime's file type.
    Mov,
}

impl Container {
    /// The container `path`'s extension names, if it names one.
    pub fn of(path: &Path) -> Option<Container> {
        path.extension()?
            .to_str()?
            .to_ascii_lowercase()
            .parse()
            .ok()
    }

    /// Whether the container holds `codec`: MP4 every codec but ProRes,
    /// QuickTime every codec but AV1.
    pub fn holds(self, codec: Codec) -> bool {
        match self {
            Container::Mp4 => !codec.keeps_alpha(),
            Container::Mov => codec != Codec::Av1,
        }
    }
}

/// Writes frames into a video file: each frame is converted, compressed and
/// written as it comes, so memory does not grow with the video's length.
/// The file appears under its name only once [`VideoWriter::finish`] has
/// written all of it.
pub struct VideoWriter {
    spec: VideoSpec,
    encoder: Box<dyn VideoEncoder>,
    /// Whether frames keep alpha for the codec.
    keeps_alpha: bool,
    muxer: Box<dyn Muxer>,
    /// The container, which picks how sound is encoded.
    container: Container,
    /// The sound, encoded when it was given.
    sound: Option<EncodedSound>,
    /// The file being written, renamed to `path` when finished.
    partial: PathBuf,
    path: PathBuf,
    frames: u64,
}

impl VideoWriter {
    /// A writer of `spec`-sized frames into `path`, whose extension picks
    /// the container, compressed with `codec` at `quality`.
    ///
    /// # Errors
    /// A diagnostic for an extension that names no container, a container
    /// that does not hold the codec, a quality the codec does not offer, a
    /// size or rate the codec refuses, or a file that cannot be created.
    pub fn create(
        path: &Path,
        spec: VideoSpec,
        codec: Codec,
        quality: VideoQuality,
    ) -> Result<VideoWriter> {
        let container = Container::of(path).ok_or_else(|| {
            Diagnostic::unknown(
                "video file extension",
                path.extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                <Container as strum::VariantNames>::VARIANTS,
            )
        })?;
        if !(spec.fps.is_finite() && spec.fps > 0.0) || spec.width == 0 || spec.height == 0 {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("a {}x{} video at {} fps", spec.width, spec.height, spec.fps),
            )
            .expected("a size above zero and a frame rate above zero"));
        }
        if !container.holds(codec) {
            let fits: Vec<String> = <Container as strum::VariantNames>::VARIANTS
                .iter()
                .filter(|name| {
                    name.parse()
                        .is_ok_and(|other: Container| other.holds(codec))
                })
                .map(|name| format!("`.{name}`"))
                .collect();
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("{} in a `.{container}` file", codec.label()),
            )
            .expected(format!(
                "a {} file for {}",
                fits.join(" or "),
                codec.label()
            )));
        }
        if !codec.qualities().contains(&quality) {
            return Err(Diagnostic::unknown(
                format!("video quality for {}", codec.label()),
                quality,
                codec.qualities(),
            ));
        }
        let encoder: Box<dyn VideoEncoder> = match codec {
            #[cfg(target_os = "macos")]
            Codec::H264 => Box::new(crate::toolbox::Toolbox::new(spec, System::H264, quality)?),
            #[cfg(target_os = "macos")]
            Codec::Hevc => Box::new(crate::toolbox::Toolbox::new(spec, System::Hevc, quality)?),
            #[cfg(target_os = "macos")]
            Codec::ProRes => Box::new(crate::toolbox::Toolbox::new(
                spec,
                System::ProRes4444,
                quality,
            )?),
            Codec::Av1 => Box::new(crate::av1::Av1::new(spec, quality)?),
        };
        let partial = partial_path(path);
        let muxer: Box<dyn Muxer> = match container {
            Container::Mp4 | Container::Mov => {
                Box::new(crate::mp4::Mp4::create(&partial, spec, container)?)
            }
        };
        Ok(VideoWriter {
            spec,
            encoder,
            keeps_alpha: codec.keeps_alpha(),
            muxer,
            container,
            sound: None,
            partial,
            path: path.to_owned(),
            frames: 0,
        })
    }

    /// Gives the video `track` as its sound, encoded now as the container
    /// holds it ([`encode_sound`]); it is written when the file is finished.
    ///
    /// # Errors
    /// As [`encode_sound`].
    pub fn set_sound(&mut self, track: &Soundtrack) -> Result<()> {
        self.sound = Some(encode_sound(track, self.container)?);
        Ok(())
    }

    /// Converts, compresses and writes the next frame, straight-alpha RGBA8
    /// pixels at the writer's size.
    ///
    /// # Errors
    /// A diagnostic for pixels of another size, or from the codec or the
    /// container.
    pub fn push(&mut self, rgba: &[u8]) -> Result<()> {
        let (w, h) = (self.spec.width, self.spec.height);
        let expected = w as usize * h as usize * 4;
        if rgba.len() != expected {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("{} bytes for a frame of a {w}x{h} video", rgba.len()),
            )
            .expected(format!("{expected} bytes: four per pixel")));
        }
        let frame = match self.keeps_alpha {
            true => Frame::Ayuv16(to_ayuv16(rgba, w, h)),
            false => Frame::Yuv420(to_planes(rgba, w, h)),
        };
        for packet in self.encoder.encode(&frame)? {
            self.muxer.write(&packet)?;
        }
        self.frames += 1;
        Ok(())
    }

    /// Ends the stream, finishes the file and moves it to its name. Returns
    /// how many frames it holds.
    ///
    /// # Errors
    /// A diagnostic from the codec or the container, or when the file cannot
    /// be moved to its name.
    pub fn finish(mut self) -> Result<u64> {
        for packet in self.encoder.finish()? {
            self.muxer.write(&packet)?;
        }
        self.muxer
            .finish(&self.encoder.config(), self.sound.as_ref())?;
        fs::rename(&self.partial, &self.path)
            .map_err(|e| Diagnostic::io("write", &self.path, &e))?;
        Ok(self.frames)
    }
}

/// Where the video is written until it is whole: `launch.mp4.partial`.
fn partial_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".partial");
    PathBuf::from(name)
}

#[cfg(test)]
#[path = "../tests/unit/video.rs"]
mod tests;
