//! Learned models for nakkash bakes ([D80] to [D88]).
//!
//! - **Models.** The [`lock`] names every model by what it does and pins
//!   every file it needs; the [`store`](ModelStore) keeps those files by
//!   hash; [`onnx`] reads a graph before anything runs it, and refuses one
//!   that is not plain ONNX ([D84]).
//! - **Graphs.** A pipeline runs graphs through a [`Backend`], which opens
//!   a locked graph where the lock places it and runs it on [`Tensor`]s.
//!   The ONNX Runtime backend lives in `nakkash-onnx`; tests use small
//!   backends of their own.
//! - **Pipelines.** The object segmenter with its memory kept here, the
//!   edge refinement, the optional flow smoothing ([D87]) and the tracks
//!   ([D88]), each writing what it makes as a baked asset.
//!
//! [D80]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d80-learned-models-run-in-a-bake-never-in-a-render
//! [D84]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d84-models-are-untrusted-input
//! [D87]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d87-an-objects-matte-is-its-segmentation-refined-at-its-edge
//! [D88]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d88-a-track-is-baked-once-and-applied-as-keys-on-pivots

#[cfg(feature = "fetch")]
pub mod fetch;
pub mod flow;
pub mod frames;
mod graph;
pub mod image;
pub mod lock;
pub mod matte;
mod models;
pub mod onnx;
pub mod quality;
pub mod segment;
mod tensor;

pub use graph::{Backend, Graph, OpenGraph, Placement};
pub use lock::ModelLock;
pub use models::{FETCH_COMMAND, FileState, IMPORT_COMMAND, Imported, ModelStore};
pub use tensor::{Data, Outputs, Tensor};
