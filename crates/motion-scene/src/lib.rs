//! Steps 2 and 3: [`compile`] a [`motion_format::Scene`] into a [`SceneGraph`] in pixels
//! and frames, then [`evaluate`] it at any frame without drawing.

mod animate;
mod compile;
mod curve;
mod evaluate;
mod graph;
mod shape;
mod state;
mod text;
mod track;

pub use animate::{COUNTS, GlyphParts, GlyphState, Parts};
pub use compile::compile;
pub use curve::Ease;
pub use evaluate::{evaluate, evaluate_comp, local_transform};
pub use graph::{CompGraph, ItemNode, Node, NodeContent, PropertyTrack, SceneGraph, Timing};
pub use shape::{ShapeDraw, outline};
pub use state::{ContentState, FrameState, ItemState, NodeState};
pub use text::{TextNode, set as set_text};
pub use track::{Key, KeySpec, Track};
