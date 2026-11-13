//! `motion-scene`: the scene graph of motion.
//!
//! This crate is step 2 of the pipeline (Compile): it turns a validated
//! [`motion_format::Scene`] into a [`SceneGraph`] where every layer has a
//! resolved centre position, a size in pixels and an entrance expressed in
//! frames. Later steps (evaluate, query, render) read the graph and never need
//! to know about shorthand strings.

mod compile;
pub mod curve;
mod graph;
mod text;

pub use compile::compile;
pub use graph::{Enter, Node, NodeKind, SceneGraph};
pub use text::{ApproxTextMeasure, TextMeasure};
