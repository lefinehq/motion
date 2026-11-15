//! H.264, HEVC and ProRes 4444 from the system's VideoToolbox on macOS ([D48]), behind
//! [`VideoEncoder`].
//!
//! [D48]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d48-unsafe-code-lives-only-in-platform-bindings

use motion_format::{Diagnostic, Kind, Result};
use motion_videotoolbox::{
    Codec as System, Encoder, Error, Image, Planes as SystemPlanes, Settings,
};

use crate::video::{DecoderConfig, Frame, Packet, VideoEncoder, VideoQuality, VideoSpec};

/// A keyframe at least this often, in frames, so a player can seek.
const MAX_KEYFRAME_INTERVAL: u32 = 240;

/// Each quality's setting, from 0 (smallest) to 1 (closest to the frames).
fn quality_of(quality: VideoQuality) -> f32 {
    match quality {
        VideoQuality::Draft => 0.35,
        VideoQuality::Web => 0.8,
        VideoQuality::Master => 0.9,
    }
}

/// The codec's name as people write it.
fn label(codec: System) -> &'static str {
    match codec {
        System::H264 => "H.264",
        System::Hevc => "HEVC",
        System::ProRes4444 => "ProRes 4444",
    }
}

/// The system's encoder for one codec.
pub(crate) struct Toolbox {
    encoder: Encoder,
    codec: System,
}

impl Toolbox {
    /// An encoder of `spec`-sized frames in `codec` at `quality`.
    pub(crate) fn new(spec: VideoSpec, codec: System, quality: VideoQuality) -> Result<Toolbox> {
        let settings = Settings {
            codec,
            width: spec.width,
            height: spec.height,
            fps: spec.fps,
            quality: quality_of(quality),
            keyframe_interval: MAX_KEYFRAME_INTERVAL,
        };
        let encoder = Encoder::new(settings).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!(
                    "this Mac cannot encode {} at {}x{}: {e}",
                    label(codec),
                    spec.width,
                    spec.height
                ),
            )
            .expected("another codec, such as `av1`")
        })?;
        Ok(Toolbox { encoder, codec })
    }

    /// The system's failure, as a diagnostic.
    fn failure(&self, error: &Error) -> Diagnostic {
        Diagnostic::new(
            Kind::Invalid,
            format!("the system's {} encoder: {error}", label(self.codec)),
        )
    }
}

impl VideoEncoder for Toolbox {
    fn encode(&mut self, frame: &Frame) -> Result<Vec<Packet>> {
        let image = match frame {
            Frame::Yuv420(planes) => Image::Yuv420(SystemPlanes {
                y: &planes.y,
                cb: &planes.cb,
                cr: &planes.cr,
            }),
            Frame::Ayuv16(frame) => Image::Ayuv16(&frame.samples),
        };
        let packets = self.encoder.encode(image).map_err(|e| self.failure(&e))?;
        Ok(packets.into_iter().map(packet).collect())
    }

    fn finish(&mut self) -> Result<Vec<Packet>> {
        let packets = self.encoder.finish().map_err(|e| self.failure(&e))?;
        Ok(packets.into_iter().map(packet).collect())
    }

    fn config(&self) -> DecoderConfig {
        let record = self.encoder.config().unwrap_or_default();
        match self.codec {
            System::H264 => DecoderConfig::Avc { record },
            System::Hevc => DecoderConfig::Hevc { record },
            System::ProRes4444 => DecoderConfig::ProRes,
        }
    }
}

fn packet(packet: motion_videotoolbox::Packet) -> Packet {
    Packet {
        data: packet.data,
        frame: packet.frame,
        keyframe: packet.keyframe,
    }
}
