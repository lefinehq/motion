//! Reads `.aep` projects. The file is a RIFX chunk tree (`rifx`) of fixed
//! binary records (`record`); [`decode`] walks it into a typed [`Project`]
//! and keeps whatever it cannot decode yet as an opaque value with its reason.
//!
//! It reads bytes only: it never touches files, pixels or the scene format.
//! The layout follows the open format notes in `aep_parser`'s
//! [`file_format.md`](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md).

mod decode;
mod model;
mod record;
mod rifx;

#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;

pub use decode::decode;
pub use model::{
    BlendMode, Child, Color, Composition, Expression, Footage, Group, Item, ItemKind, Layer,
    LayerKind, Project, Property, Reason, Source, Span, Stream, Switches, TrackMatte, Value,
    Version,
};
