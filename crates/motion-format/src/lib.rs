//! `motion-format`: the scene format of motion.
//!
//! This crate is step 1 of the pipeline (Read). It reads scene JSON with
//! serde into a typed [`Scene`] ([`parse_document`], [`scene_from_value`]),
//! validating it in one pass with errors that name the key and the accepted
//! form; expands the short strings people and agents write (`"0.4s"`,
//! `"#0B0D12"`, `"1080p"`) into typed values; and writes a scene back out
//! ([`write_document`]) keeping what the author wrote. It never touches
//! pixels or files.

pub mod anim;
mod curve;
mod doc;
pub mod edit_word;
mod number;
mod parse;
pub mod preset;
pub mod property;
mod scene;
pub mod shorthand;
mod table;
pub mod units;
mod validate;
mod write;

pub use curve::{Curve, NAMED_CURVES};
pub use number::fmt_num;
pub use parse::{LAYER_KEYS, SCENE_KEYS, key, parse_document, scene_from_value};
pub use scene::{Content, Document, Layer, Scene};
pub use write::write_document;
