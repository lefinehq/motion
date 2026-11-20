//! AV1 through `rav1e`: pure Rust under BSD-2-Clause, built without its
//! assembly, so it runs on every platform with nothing installed ([D46]).
//!
//! [D46]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait

use nakkash_format::{Diagnostic, Kind, Result};
use rav1e::color::{
    ChromaSampling, ColorDescription, ColorPrimaries, MatrixCoefficients, PixelRange,
    TransferCharacteristics,
};
use rav1e::config::SpeedSettings;
use rav1e::data::{EncoderStatus, FrameType, Rational};
use rav1e::{Config, Context, EncoderConfig};

use crate::video::{DecoderConfig, Frame, Packet, VideoEncoder, VideoQuality, VideoSpec};

/// A keyframe at least this often, in frames, so a player can seek.
const MAX_KEYFRAME_INTERVAL: u64 = 240;

/// An AV1 encoder.
pub(crate) struct Av1 {
    context: Context<u8>,
    /// The sequence header OBU of the first keyframe, for the container.
    sequence_header: Option<Vec<u8>>,
}

/// Each quality's speed preset, from 0 (slowest, smallest) to 10 (fastest),
/// and quantizer, from 0 (lossless) to 255.
fn settings(quality: VideoQuality) -> (u8, usize) {
    match quality {
        // Quick to make, for looking at timing and layout.
        VideoQuality::Draft => (10, 150),
        // Text stays crisp; the measured gap to the drawn frames is under
        // what the eye sees on screen.
        VideoQuality::Web => (8, 80),
        // For handing on to be edited or encoded again.
        VideoQuality::Master => (6, 30),
    }
}

/// How many tiles a frame is cut into, which are encoded in parallel. It
/// follows the frame's size, never the machine, so the file is the same
/// whatever the number of cores.
fn tiles(spec: VideoSpec) -> usize {
    match u64::from(spec.width) * u64::from(spec.height) {
        pixels if pixels >= 3840 * 2160 => 16,
        pixels if pixels >= 1280 * 720 => 8,
        pixels if pixels >= 640 * 360 => 2,
        _ => 1,
    }
}

impl Av1 {
    /// An encoder of `spec`-sized frames at `quality`, tagged BT.709
    /// limited range, on as many threads as the process works on
    /// ([`nakkash_machine::threads`]).
    pub(crate) fn new(spec: VideoSpec, quality: VideoQuality) -> Result<Av1> {
        Av1::with_threads(spec, quality, nakkash_machine::threads())
    }

    /// As [`Av1::new`], on `threads` threads, or every core when zero.
    fn with_threads(spec: VideoSpec, quality: VideoQuality, threads: usize) -> Result<Av1> {
        let (speed, quantizer) = settings(quality);
        let encoder = EncoderConfig {
            width: spec.width as usize,
            height: spec.height as usize,
            time_base: time_base(spec.fps),
            bit_depth: 8,
            chroma_sampling: ChromaSampling::Cs420,
            pixel_range: PixelRange::Limited,
            color_description: Some(ColorDescription {
                color_primaries: ColorPrimaries::BT709,
                transfer_characteristics: TransferCharacteristics::BT709,
                matrix_coefficients: MatrixCoefficients::BT709,
            }),
            speed_settings: SpeedSettings::from_preset(speed),
            quantizer,
            tiles: tiles(spec),
            max_key_frame_interval: MAX_KEYFRAME_INTERVAL,
            ..EncoderConfig::default()
        };
        // The tiles are fixed by the frame's size, so the same frames give
        // the same bytes on any number of threads.
        let context = Config::new()
            .with_encoder_config(encoder)
            .with_threads(threads)
            .new_context()
            .map_err(|e| {
                Diagnostic::new(
                    Kind::Invalid,
                    format!("the AV1 encoder refuses its settings: {e}"),
                )
                .expected("a video size and frame rate AV1 can hold")
            })?;
        Ok(Av1 {
            context,
            sequence_header: None,
        })
    }

    /// Every packet the encoder has ready.
    fn drain(&mut self) -> Result<Vec<Packet>> {
        let mut packets = Vec::new();
        loop {
            match self.context.receive_packet() {
                Ok(packet) => {
                    let keyframe = packet.frame_type == FrameType::KEY;
                    if keyframe && self.sequence_header.is_none() {
                        self.sequence_header = sequence_header(&packet.data).map(<[u8]>::to_vec);
                    }
                    packets.push(Packet {
                        data: packet.data,
                        frame: packet.input_frameno,
                        keyframe,
                    });
                }
                Err(EncoderStatus::Encoded) => {}
                Err(EncoderStatus::NeedMoreData | EncoderStatus::LimitReached) => {
                    return Ok(packets);
                }
                Err(other) => return Err(failure(other)),
            }
        }
    }
}

impl VideoEncoder for Av1 {
    fn encode(&mut self, frame: &Frame) -> Result<Vec<Packet>> {
        let Frame::Yuv420(planes) = frame else {
            return Err(Diagnostic::new(
                Kind::Invalid,
                "the AV1 encoder takes 4:2:0 frames without alpha",
            ));
        };
        let mut frame = self.context.new_frame();
        let sources = [
            (&planes.y, planes.width),
            (&planes.cb, planes.chroma_width()),
            (&planes.cr, planes.chroma_width()),
        ];
        for (plane, (data, stride)) in frame.planes.iter_mut().zip(sources) {
            plane.copy_from_raw_u8(data, stride as usize, 1);
        }
        self.context.send_frame(frame).map_err(failure)?;
        self.drain()
    }

    fn finish(&mut self) -> Result<Vec<Packet>> {
        self.context.flush();
        self.drain()
    }

    fn config(&self) -> DecoderConfig {
        // The record's four bytes, then the sequence header as its one
        // configuration OBU, which some players need before the first frame.
        let mut record = self.context.container_sequence_header();
        record.extend_from_slice(self.sequence_header.as_deref().unwrap_or_default());
        DecoderConfig::Av1 { record }
    }
}

/// The sequence header OBU among a temporal unit's OBUs, if it has one.
/// Each OBU is a header byte, an extension byte when flagged, and a LEB128
/// size when flagged; rav1e always writes the size.
fn sequence_header(mut unit: &[u8]) -> Option<&[u8]> {
    const SEQUENCE_HEADER: u8 = 1;
    while let [header, ..] = *unit {
        let kind = (header >> 3) & 0xf;
        let extension = usize::from(header & 0b100 != 0);
        if header & 0b10 == 0 {
            return None;
        }
        let (size, length) = leb128(unit.get(1 + extension..)?)?;
        let end = (1 + extension + length).checked_add(size)?;
        let obu = unit.get(..end)?;
        if kind == SEQUENCE_HEADER {
            return Some(obu);
        }
        unit = &unit[end..];
    }
    None
}

/// A LEB128 number and how many bytes it took, at most eight.
fn leb128(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0usize;
    for (i, byte) in bytes.iter().take(8).enumerate() {
        value |= usize::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

/// The time base of `fps`: one frame's length as a fraction of a second,
/// to a thousandth of a frame per second, so 29.97 is exact.
fn time_base(fps: f64) -> Rational {
    Rational::new(1000, (fps * 1000.0).round() as u64)
}

/// A status the encoder should not return here, as a diagnostic.
fn failure(status: EncoderStatus) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("the AV1 encoder failed: {status:?}"))
}

#[cfg(test)]
#[path = "../tests/unit/av1.rs"]
mod tests;
