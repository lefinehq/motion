//! H.264 and HEVC from the system's VideoToolbox, on macOS ([D48]).
//!
//! The system's encoders run on the media engine where the Mac has one, and
//! in software where it has not, and Apple's licence covers the codecs
//! ([D13]). This crate wraps one compression session in a safe [`Encoder`]:
//! planes in, packets out. It is the one crate of the workspace allowed
//! unsafe code, for the calls into the system's frameworks, and every
//! unsafe block says why it is sound. It knows nothing of motion's own
//! types; `motion-encode` puts it behind its codec trait.
//!
//! [D13]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d13-codec-licence-policy
//! [D48]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d48-unsafe-code-lives-only-in-platform-bindings
#![cfg(target_os = "macos")]

mod encoder;

pub use encoder::{Codec, Encoder, Error, Packet, Planes, Settings};
