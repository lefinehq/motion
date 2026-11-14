//! Steps 2 and 3: [`compile`] a [`motion_format::Scene`] into a [`SceneGraph`] in pixels
//! and frames, then [`evaluate`] it at any frame without drawing.

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
