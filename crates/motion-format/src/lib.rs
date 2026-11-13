//! `motion-format`: the scene format of motion.
//!
//! This crate is step 1 of the pipeline (Read). It parses scene JSON into a
//! typed [`Scene`] ([`parse_document`], [`scene_from_value`]), validates it
//! with errors that name the key and the accepted form, expands the short
//! strings people and agents write (`"0.4s"`, `"#0B0D12"`, `"1080p"`) into
//! typed values, and writes a scene back out ([`write_document`]) keeping
//! what the author wrote. It never touches pixels or files.

mod error;
mod number;
mod parse;
mod scene;
pub mod shorthand;
pub mod units;
mod write;

pub use error::{FormatError, ShorthandError};
pub use number::fmt_num;
pub use parse::{LAYER_KEYS, SCENE_KEYS, parse_document, scene_from_value};
pub use scene::{Content, Document, Layer, Scene};
pub use write::write_document;
