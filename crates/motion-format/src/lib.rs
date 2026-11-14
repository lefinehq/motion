//! The motion scene format: read scene JSON into a typed [`Scene`], expand its
//! shorthand strings (`"0.4s"`, `"#0B0D12"`, `"1080p"`) and write it back as
//! the author wrote it. Also home to [`Diagnostic`] and the shared [`grammar`].

pub mod anim;
mod curve;
mod de;
mod diagnostic;
mod doc;
pub mod edit_word;
pub mod grammar;
pub mod item;
mod number;
mod parse;
pub mod path;
pub mod preset;
pub mod property;
mod scene;
pub mod shorthand;
mod table;
pub mod text;
pub mod units;
mod validate;
pub mod value;
mod write;

pub use curve::{Curve, NAMED_CURVES};
pub use diagnostic::{Diagnostic, Kind, Result};
pub use item::{Item, ItemKind, Role};
pub use number::fmt_num;
pub use parse::{COMP_KEYS, LAYER_KEYS, SCENE_KEYS, key, layer_keys, parse_document};
pub use path::{Path, Vertex};
pub use scene::{Comp, Content, Document, Layer, Scene};
pub use value::{Value, ValueType};
pub use write::write_document;

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
