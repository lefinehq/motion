//! [`Made`]: what made a baked asset.

use serde::{Deserialize, Serialize};

use nakkash_format::Sha256;

/// What made a baked asset ([D80]): enough to tell two bakes of one request
/// apart, and to make it again on the same machine to the bit.
///
/// [D80]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d80-learned-models-run-in-a-bake-never-in-a-render
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Made {
    /// The version of nakkash that baked it.
    pub nakkash: String,
    /// The operating system and processor it ran on, such as
    /// `macos-aarch64`.
    pub platform: String,
    /// The inference runtime, when a model ran.
    pub runtime: Option<Runtime>,
    /// The threads each graph ran on.
    pub threads: usize,
    /// Every graph that ran, in the order it first ran.
    pub graphs: Vec<GraphRun>,
}

/// The inference runtime a bake loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    /// Its version, such as `1.31.0`.
    pub version: String,
    /// The hash of the library loaded.
    pub sha256: Sha256,
}

/// One graph a bake ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphRun {
    /// The model's key in the lock, such as `segment.video`.
    pub model: String,
    /// The graph's name within the model, such as `image-encoder`.
    pub graph: String,
    /// The hash of the graph's file.
    pub sha256: Sha256,
    /// Where it ran: `cpu`, or `coreml` on macOS.
    pub backend: String,
}

impl Made {
    /// A record for this build of nakkash on this machine, with nothing run
    /// yet.
    pub fn here(threads: usize) -> Self {
        Self {
            nakkash: env!("CARGO_PKG_VERSION").to_owned(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            runtime: None,
            threads,
            graphs: Vec::new(),
        }
    }

    /// Records that `run` ran, once however often it runs.
    pub fn ran(&mut self, run: GraphRun) {
        if !self.graphs.contains(&run) {
            self.graphs.push(run);
        }
    }
}
