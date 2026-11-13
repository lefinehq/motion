//! `motion-format`: the scene format of motion.
//!
//! This crate is step 1 of the pipeline (Read): it turns the short strings
//! people and agents write in a scene (`"0.4s"`, `"#0B0D12"`, `"1080p"`) into
//! typed values. It never touches pixels and has no knowledge of drawing.

mod error;
mod number;
pub mod shorthand;
pub mod units;

pub use error::ShorthandError;
pub use number::fmt_num;
mod parse;
mod scene;
mod write;

pub use error::FormatError;
pub use parse::{LAYER_KEYS, SCENE_KEYS, parse_document, scene_from_value};
pub use scene::{Content, Document, Layer, Scene};
pub use write::write_document;
