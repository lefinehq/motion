//! [`ModelLock`]: every model nakkash may run, and every file it needs,
//! pinned by hash ([D82]), with its licence ([D83]).
//!
//! [D82]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d82-a-model-lock-and-a-content-addressed-model-store
//! [D83]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d83-only-models-licensed-for-commercial-use

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use nakkash_format::{Diagnostic, Kind, Result, Sha256};

use crate::Placement;

/// The version of the lock format this crate reads.
pub const MODEL_LOCK_VERSION: u32 = 1;

/// The lock built into nakkash.
const BUILTIN: &str = include_str!("../models.lock");

/// Every model nakkash may run, by a key that says what it does, and the
/// runtime that runs them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelLock {
    /// The lock format's version, [`MODEL_LOCK_VERSION`].
    pub lock: u32,
    /// The inference runtime ([D81]).
    ///
    /// [D81]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d81-one-inference-runtime-loaded-at-run-time-from-a-locked-file
    pub runtime: LockedRuntime,
    /// The models, by key, such as `segment.video`.
    pub models: BTreeMap<String, LockedModel>,
    /// Models that must never enter the lock, by key, with the reason.
    pub refused: BTreeMap<String, String>,
}

/// The runtime: one package from its vendor, and the library inside it for
/// each platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedRuntime {
    /// Its version, such as `1.31.0`.
    pub version: String,
    /// Its licence.
    pub licence: Licence,
    /// The package that holds the libraries.
    pub package: Package,
    /// The library for each platform, by `<os>-<arch>` as Rust names them,
    /// such as `macos-aarch64`.
    pub libraries: BTreeMap<String, Member>,
}

/// A downloaded archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    /// Where it is downloaded from.
    pub url: String,
    /// Its hash, under which the store keeps it.
    pub sha256: Sha256,
    /// Its SHA-512 in base64, as the package registry publishes it: a
    /// second check, against a hash we did not compute.
    pub sha512: String,
    /// Its size in bytes.
    pub size: u64,
}

/// A file inside a [`Package`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    /// Its path inside the package.
    pub path: String,
    /// Its hash.
    pub sha256: Sha256,
    /// Its size in bytes.
    pub size: u64,
}

/// One model: the graphs and constant tensors it needs, and its licence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedModel {
    /// What it does, in a sentence.
    pub does: String,
    /// Its licence, as its upstream states it.
    pub licence: Licence,
    /// Its graphs, by name.
    pub graphs: BTreeMap<String, LockedGraph>,
    /// Its constant tensors, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub constants: BTreeMap<String, LockedConstant>,
}

/// One ONNX graph of a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedGraph {
    /// Its file's hash.
    pub sha256: Sha256,
    /// Its file's size in bytes.
    pub size: u64,
    /// Where it runs.
    pub placement: Placement,
    /// How the file is obtained.
    pub obtain: Obtain,
}

/// One constant tensor a model's host code needs: little-endian 32-bit
/// floats in an `.npy` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedConstant {
    /// Its file's hash.
    pub sha256: Sha256,
    /// Its file's size in bytes.
    pub size: u64,
    /// Its shape.
    pub shape: Vec<usize>,
    /// How the file is obtained.
    pub obtain: Obtain,
}

/// How a locked file is obtained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum Obtain {
    /// Downloaded from the upstream's own release.
    Download {
        /// The release's URL.
        url: String,
    },
    /// Exported by us from the upstream's weights, outside nakkash, offline
    /// ([D84]), and taken in by `nakkash ml models import`.
    ///
    /// [D84]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d84-models-are-untrusted-input
    Export(Export),
}

/// How an exported file was made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    /// The upstream weights, as an exact URL.
    pub weights: String,
    /// Their hash.
    pub weights_sha256: Sha256,
    /// The upstream code the exporter imports, at an exact commit.
    pub code: String,
    /// The exporter script's file name and hash.
    pub exporter: String,
    /// The toolchain it ran with.
    pub toolchain: String,
    /// The largest difference measured between the graph's outputs and the
    /// upstream module's, in eval mode.
    pub parity_max_abs: f64,
}

/// A licence, as its upstream states it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Licence {
    /// Its SPDX id, such as `Apache-2.0`.
    pub spdx: String,
    /// The upstream page it was read from: never a mirror's.
    pub url: String,
    /// What it allows.
    pub class: LicenceClass,
    /// A notice the licence asks to be shown, for a `conditions` licence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// What a licence allows ([D83]).
///
/// [D83]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d83-only-models-licensed-for-commercial-use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum LicenceClass {
    /// Commercial use and changes allowed.
    Ok,
    /// Allowed, with attribution or terms of use.
    Conditions,
    /// Not for commercial use: refused.
    NonCommercial,
    /// Copyleft: refused.
    Copyleft,
}

/// A file the lock pins, wherever it sits in the lock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pinned<'a> {
    /// What it is, for messages: `segment.video image-encoder`.
    pub name: &'a str,
    /// The part of it after the model's key, empty for the runtime.
    pub part: &'a str,
    /// Its hash.
    pub sha256: &'a Sha256,
    /// Its size.
    pub size: u64,
    /// Whether it is an ONNX graph, which is read before it is kept.
    pub graph: bool,
    /// How it is obtained, `None` for a member of the runtime's package.
    pub obtain: Option<&'a Obtain>,
}

impl ModelLock {
    /// The lock built into nakkash.
    ///
    /// # Panics
    /// When the built-in lock does not read, which a test rules out.
    pub fn builtin() -> Self {
        Self::read(BUILTIN).expect("the built-in model lock reads")
    }

    /// The lock in `text`, checked: every licence commercial-clean, every
    /// model with a graph, no refused model in it.
    ///
    /// # Errors
    /// A [`Kind::Invalid`] diagnostic when it is not a model lock of this
    /// version, and a [`Kind::Conflict`] one naming the model when a model
    /// is not licensed for commercial use or is on the refused list.
    pub fn read(text: &str) -> Result<Self> {
        let lock: ModelLock = serde_json::from_str(text)
            .map_err(|e| Diagnostic::new(Kind::Invalid, format!("not a model lock: {e}")))?;
        if lock.lock != MODEL_LOCK_VERSION {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "a model lock of version {}, not {MODEL_LOCK_VERSION}",
                    lock.lock
                ),
            ));
        }
        let licences = std::iter::once(("the runtime", &lock.runtime.licence))
            .chain(lock.models.iter().map(|(k, m)| (k.as_str(), &m.licence)));
        for (name, licence) in licences {
            licence_allowed(name, licence)?;
        }
        for (key, model) in &lock.models {
            if let Some(why) = lock.refused.get(key) {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!("model `{key}` is refused: {why}"),
                ));
            }
            if model.graphs.is_empty() {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("model `{key}` has no graph"),
                ));
            }
        }
        Ok(lock)
    }

    /// The model with `key`.
    ///
    /// # Errors
    /// A [`Kind::UnknownName`] diagnostic listing the lock's keys.
    pub fn model(&self, key: &str) -> Result<&LockedModel> {
        self.models.get(key).ok_or_else(|| {
            Diagnostic::unknown("model", key, self.models.keys().map(String::as_str))
        })
    }

    /// The runtime library for this platform.
    ///
    /// # Errors
    /// A [`Kind::Missing`] diagnostic naming the platforms there is one for.
    pub fn library_here(&self) -> Result<&Member> {
        let here = platform();
        self.runtime.libraries.get(&here).ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!(
                    "the inference runtime has no library for {here}; it has one for {}",
                    self.runtime
                        .libraries
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })
    }

    /// Every file of every model, graphs first in each, in key order.
    pub fn files(&self) -> Vec<Pinned<'_>> {
        let mut files = Vec::new();
        for (key, model) in &self.models {
            for (name, graph) in &model.graphs {
                files.push(Pinned {
                    name: key,
                    part: name,
                    sha256: &graph.sha256,
                    size: graph.size,
                    graph: true,
                    obtain: Some(&graph.obtain),
                });
            }
            for (name, constant) in &model.constants {
                files.push(Pinned {
                    name: key,
                    part: name,
                    sha256: &constant.sha256,
                    size: constant.size,
                    graph: false,
                    obtain: Some(&constant.obtain),
                });
            }
        }
        files
    }

    /// The file the lock pins under `hash`, if any.
    pub fn pinned(&self, hash: &Sha256) -> Option<Pinned<'_>> {
        self.files().into_iter().find(|f| f.sha256 == hash)
    }
}

/// This platform, as the lock names platforms: `macos-aarch64`.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Refuses `licence` of `name` unless it allows commercial use.
fn licence_allowed(name: &str, licence: &Licence) -> Result<()> {
    match licence.class {
        LicenceClass::Ok => Ok(()),
        LicenceClass::Conditions if licence.notice.is_some() => Ok(()),
        LicenceClass::Conditions => Err(Diagnostic::new(
            Kind::Missing,
            format!(
                "`{name}` is under {} with conditions, and the lock gives no notice for it",
                licence.spdx
            ),
        )),
        class => Err(Diagnostic::new(
            Kind::Conflict,
            format!(
                "`{name}` is under {}, which is {class}: nakkash takes only models licensed for commercial use (D83)",
                licence.spdx
            ),
        )),
    }
}

#[cfg(test)]
#[path = "../tests/unit/lock.rs"]
pub(crate) mod tests;
