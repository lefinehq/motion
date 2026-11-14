//! The motion scene format: read scene JSON into a typed [`Scene`], validate
//! it, expand shorthand strings (`"0.4s"`, `"#0B0D12"`, `"1080p"`) and write
//! it back keeping what the author wrote. Also home to [`Diagnostic`], the one
//! error type of every motion crate, and the [`grammar`] pieces every text
//! grammar shares.

pub mod anim;
mod curve;
mod diagnostic;
mod doc;
pub mod edit_word;
pub mod grammar;
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
pub use diagnostic::{Diagnostic, Kind, Result};
pub use number::fmt_num;
pub use parse::{LAYER_KEYS, SCENE_KEYS, key, parse_document};
pub use scene::{Content, Document, Layer, Scene};
pub use write::write_document;
