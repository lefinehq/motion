//! [`Graph`] and [`Backend`]: how pipelines run graphs without knowing the
//! runtime that runs them.

use std::path::PathBuf;

use nakkash_baked::{GraphRun, Runtime};
use nakkash_format::{Result, Sha256};

use crate::{Outputs, Tensor};

/// Where a graph runs ([D81]).
///
/// [D81]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d81-one-inference-runtime-loaded-at-run-time-from-a-locked-file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    /// On the CPU.
    Cpu,
    /// On CoreML, on macOS, for a graph whose input shapes are fixed; on the
    /// CPU elsewhere.
    CoreMl,
}

impl Placement {
    /// The backend's name in a bake's record: `cpu` or `coreml`, as it ran
    /// on this machine.
    pub fn here(self) -> &'static str {
        match self {
            Placement::CoreMl if cfg!(target_os = "macos") => "coreml",
            _ => "cpu",
        }
    }
}

/// A locked graph to open: where its file is kept, checked, and where it
/// runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGraph {
    /// The model's key in the lock, such as `segment.video`.
    pub model: String,
    /// The graph's name within the model, such as `image-encoder`.
    pub graph: String,
    /// The graph's file in the store, its hash already checked.
    pub path: PathBuf,
    /// Its hash.
    pub sha256: Sha256,
    /// Where it runs.
    pub placement: Placement,
    /// The threads it runs on.
    pub threads: usize,
}

impl OpenGraph {
    /// What a bake records of it.
    pub fn run(&self) -> GraphRun {
        GraphRun {
            model: self.model.clone(),
            graph: self.graph.clone(),
            sha256: self.sha256,
            backend: self.placement.here().to_owned(),
        }
    }
}

/// An open graph, ready to run.
pub trait Graph: Send {
    /// The names of the inputs it takes, in its order. A pipeline gives
    /// only these: an export may drop an input the graph does not use.
    fn inputs(&self) -> Vec<String>;

    /// Runs it once on `inputs`, by name.
    ///
    /// # Errors
    /// The runtime's diagnostic when an input is missing or of the wrong
    /// shape, or the run fails.
    fn run(&mut self, inputs: Vec<(String, Tensor)>) -> Result<Outputs>;
}

/// What opens and runs graphs: an inference runtime.
pub trait Backend {
    /// Opens `graph`.
    ///
    /// # Errors
    /// The runtime's diagnostic when the graph cannot be loaded or placed.
    fn open(&self, graph: &OpenGraph) -> Result<Box<dyn Graph>>;

    /// The runtime, for a bake's record; `None` for a backend that loads
    /// none, such as a test's.
    fn runtime(&self) -> Option<Runtime>;
}
