//! Compiles a validated [`motion_format::Scene`] into a [`SceneGraph`] (sizes
//! in pixels, one keyframe track per property, in frames) and [`evaluate`]s
//! it at any frame without drawing. Nothing after this crate reads shorthand
//! strings.

mod compile;
pub mod curve;
mod evaluate;
mod graph;
mod state;
mod text;
mod track;

pub use compile::compile;
pub use evaluate::evaluate;
pub use graph::{Node, NodeKind, SceneGraph};
pub use state::{FrameState, NodeState};
pub use text::{ApproxTextMeasure, TextMeasure};
pub use track::{Key, Track};
