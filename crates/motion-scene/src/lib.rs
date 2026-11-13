//! `motion-scene`: the scene graph of motion.
//!
//! This crate is step 2 of the pipeline (Compile): it turns a validated
//! [`motion_format::Scene`] into a [`SceneGraph`] where every layer has a
//! size in pixels and a keyframe track for every property, in frames. Later
//! steps (evaluate, query, render) read the graph and never need to know
//! about shorthand strings.

mod compile;
pub mod curve;
mod graph;
mod text;
mod track;

pub use compile::compile;
pub use graph::{Node, NodeKind, SceneGraph};
pub use text::{ApproxTextMeasure, TextMeasure};
pub use track::{Key, Track};
