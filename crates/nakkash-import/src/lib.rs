//! Reads `.aep` projects. The file is a RIFX chunk tree (`rifx`) of fixed
//! binary records (`record`); [`decode`] walks it into a typed [`Project`]
//! and keeps whatever it cannot decode yet as an opaque value with its
//! reason, which [`Project::report`] counts.
//!
//! It reads bytes only: it never touches files, pixels or the scene format.
//! The layout follows the open format notes in `aep_parser`'s
//! [`file_format.md`](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md).

pub mod bake;
mod cos;
mod decode;
mod eval;
pub mod export;
pub mod map;
mod model;
mod parts;
mod record;
mod report;
mod rifx;
pub mod template;
mod tree;

#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;

pub use decode::decode;
pub use model::{
    AutoOrient, BaselineOption, BlendMode, Caps, CharacterStyle, Child, Color, ComposedLine,
    ComposedSegment, Composition, Control, ControlKind, ControlTarget, Ease, EffectDefinition,
    Expression, ExpressionEngine, Footage, Group, Interpolation, Item, ItemKind, Justification,
    Kerning, Keyframe, Layer, LayerKind, LeadingType, LineJoin, PanelControl, ParagraphStyle,
    Parameter, Path, PathStep, Project, Property, Reason, Renderer, Run, Source, Span, Spatial,
    Stream, Switches, TextDocument, TextFrame, TrackMatte, Value, Version, Vertex,
};
pub use report::Report;
