//! The pieces of a video file and the writer that joins them: a codec
//! behind [`VideoEncoder`], a container behind [`Muxer`].

use std::fs;
use std::path::{Path, PathBuf};

use motion_format::{Diagnostic, Kind, Result};

use crate::colour::{Planes, to_planes};

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

/// What a container stores so a player can set up the codec's decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecoderConfig {
    /// An AV1 codec configuration record, its sequence header included.
    Av1 {
        /// The record's bytes, as the `av1C` box holds them.
        record: Vec<u8>,
    },
}

/// A codec: planes in, in order, packets out. It is `Send` so frames drawn
/// on several threads can be handed to it in turn.
pub trait VideoEncoder: Send {
    /// Takes the next frame, and returns the packets ready so far, which may
    /// be none while the codec looks ahead.
    ///
    /// # Errors
    /// The codec's failure, as a diagnostic.
    fn encode(&mut self, planes: &Planes) -> Result<Vec<Packet>>;

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
    /// among it, and closes the file.
    ///
    /// # Errors
    /// A diagnostic when the file cannot be finished.
    fn finish(self: Box<Self>, config: &DecoderConfig) -> Result<()>;
}

/// The codecs this build has, by the name `--codec` takes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::Display, strum::EnumString, strum::VariantNames,
)]
#[strum(serialize_all = "lowercase")]
pub enum Codec {
    /// AV1, through `rav1e`, on every platform.
    Av1,
}

/// The containers, by file extension.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::Display, strum::EnumString, strum::VariantNames,
)]
#[strum(serialize_all = "lowercase")]
pub enum Container {
    /// MP4, written by this crate on `mp4-atom`'s boxes.
    Mp4,
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
}

/// Writes frames into a video file: each frame is converted, compressed and
/// written as it comes, so memory does not grow with the video's length.
/// The file appears under its name only once [`VideoWriter::finish`] has
/// written all of it.
pub struct VideoWriter {
    spec: VideoSpec,
    encoder: Box<dyn VideoEncoder>,
    muxer: Box<dyn Muxer>,
    /// The file being written, renamed to `path` when finished.
    partial: PathBuf,
    path: PathBuf,
    frames: u64,
}

impl VideoWriter {
    /// A writer of `spec`-sized frames into `path`, whose extension picks
    /// the container, compressed with `codec`.
    ///
    /// # Errors
    /// A diagnostic for an extension that names no container, a size or
    /// rate the codec refuses, or a file that cannot be created.
    pub fn create(path: &Path, spec: VideoSpec, codec: Codec) -> Result<VideoWriter> {
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
        let encoder: Box<dyn VideoEncoder> = match codec {
            Codec::Av1 => Box::new(crate::av1::Av1::new(spec)?),
        };
        let partial = partial_path(path);
        let muxer: Box<dyn Muxer> = match container {
            Container::Mp4 => Box::new(crate::mp4::Mp4::create(&partial, spec)?),
        };
        Ok(VideoWriter {
            spec,
            encoder,
            muxer,
            partial,
            path: path.to_owned(),
            frames: 0,
        })
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
        let planes = to_planes(rgba, w, h);
        for packet in self.encoder.encode(&planes)? {
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
        self.muxer.finish(&self.encoder.config())?;
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
