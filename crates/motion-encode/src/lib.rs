//! Step 5 (Encode): turns drawn frames into a video file ([D46]).
//!
//! The crate knows nothing about scenes. A [`VideoWriter`] takes frames of
//! straight RGBA8 pixels, in order, and writes a file:
//!
//! 1. [`colour`] turns each frame into BT.709 Y′CbCr 4:2:0, the planes every
//!    codec gets;
//! 2. a [`VideoEncoder`] compresses them into packets;
//! 3. a [`Muxer`] writes the packets into a container.
//!
//! Every codec follows the licence policy of [D13]: nothing GPL and no
//! patent-encumbered software encoder is compiled in.
//!
//! [D13]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d13-codec-licence-policy
//! [D46]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d46-video-is-encoded-in-a-crate-of-its-own-behind-one-codec-trait

mod av1;
pub mod colour;
mod mp4;
#[cfg(target_os = "macos")]
mod toolbox;
mod video;

pub use colour::{Ayuv16, Planes};
pub use video::{
    Codec, Container, DecoderConfig, Frame, Muxer, Packet, VideoEncoder, VideoQuality, VideoSpec,
    VideoWriter,
};
