//! Steps 2 and 3: [`compile`] a [`motion_format::Scene`] into a [`SceneGraph`] in pixels
//! and frames, then [`evaluate`] it at any frame without drawing.

mod compile;
mod curve;
mod evaluate;
mod graph;
mod state;
mod text;
mod track;

pub use compile::compile;
pub use curve::Ease;
pub use evaluate::{evaluate, local_transform};
pub use graph::{Node, NodeKind, PropertyTrack, SceneGraph, Timing};
pub use state::{FrameState, NodeState};
pub use text::{ApproxTextMeasure, TextMeasure};
pub use track::{Key, KeySpec, Track};
