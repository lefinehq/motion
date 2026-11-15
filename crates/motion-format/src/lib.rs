//! The motion scene format: read scene JSON into a typed [`Scene`], expand its
//! shorthand strings (`"0.4s"`, `"#0B0D12"`, `"1080p"`) and write it back as
//! the author wrote it. Also home to [`Diagnostic`] and the shared [`grammar`].

pub mod address;
pub mod anim;
mod curve;
mod de;
mod diagnostic;
mod doc;
pub mod edit_word;
pub mod grammar;
pub mod item;
mod number;
pub mod overrides;
mod parse;
pub mod path;
pub mod preset;
pub mod property;
pub mod roles;
mod scene;
pub mod shorthand;
pub mod slots;
pub mod spring;
mod table;
pub mod text;
pub mod units;
mod validate;
pub mod value;
pub mod when;
mod write;

pub use address::{Address, Id, Step};
pub use curve::{BACK_OVERSHOOT, Curve, ELASTIC, FAMILIES, Family, NAMED_CURVES, SHAPES, Shape};
pub use diagnostic::{Diagnostic, Kind, Place, Result};
pub use item::{Item, ItemKind, Role};
pub use number::fmt_num;
pub use parse::{COMP_KEYS, LAYER_KEYS, SCENE_KEYS, key, layer_keys, parse_document};
pub use path::{Path, Vertex};
pub use roles::RoleTarget;
pub use scene::{Comp, CompContent, Content, Document, ImageContent, Layer, Scene, VideoContent};
pub use slots::{Listed, Slot, SlotType};

/// Whether `value` is a colour in any form the scene writes one: `#RGB`,
/// `#RRGGBB`, `#RRGGBBAA`, or `[r, g, b, a]` with each channel 0 to 1.
pub fn is_color(value: &serde_json::Value) -> bool {
    text::color(value).is_ok()
}
pub use spring::Spring;
pub use value::{Value, ValueType};
pub use write::write_document;

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
