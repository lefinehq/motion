//! `motion-eval`: the value of every node at a given frame.
//!
//! This crate is step 3 of the pipeline (Evaluate): it takes the
//! [`motion_scene::SceneGraph`] from the compile step and answers "where is
//! everything, and how visible is it, at frame t?" without drawing anything.
//! The query step (checks) and the render step both build on these numbers.

mod evaluate;
mod state;

pub use evaluate::evaluate;
pub use state::{FrameState, NodeState};
