//! Steps 2 and 3: [`compile`] a [`nakkash_format::Scene`] into a [`SceneGraph`] in pixels
//! and frames, then [`evaluate`] it at any frame without drawing, and
//! [`walk`] every node of the frame at any depth.

mod animate;
mod audio;
mod boolean;
mod changes;
mod compile;
mod curve;
mod evaluate;
mod expand;
mod graph;
mod level;
mod merge;
mod rect;
mod repeater;
mod round;
mod shape;
pub mod space;
mod state;
mod text;
mod track;
mod trim;
mod walk;

pub use animate::{COUNTS, GlyphParts, GlyphState, Parts};
pub use audio::{Playing, Route, routes, routes_from};
pub use changes::changed_paths;
pub use compile::{compile, compile_with};
pub use curve::Ease;
pub use evaluate::{evaluate, evaluate_comp, local_transform};
pub use graph::{CompGraph, ItemNode, Node, NodeContent, PropertyTrack, SceneGraph, Timing};
pub use level::{Hearing, Levels, hearings};
pub use rect::content_rects;
pub use shape::{ShapeDraw, bezier, outline};
pub use state::{ContentState, FrameState, ItemState, NodeState};
pub use text::{TextNode, set as set_text, set_as_written as set_text_as_written};
pub use track::{INTEGRAL_PARTS, Integral, IntegralPiece, Key, KeySpec, PathSpan, Track};
pub use walk::{Visit, Walk, address_of, walk};
