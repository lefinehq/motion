//! [`ModelStore`]: the locked files on this machine, kept by hash ([D82]).
//!
//! [D82]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d82-a-model-lock-and-a-content-addressed-model-store

use std::fs;
use std::path::{Path, PathBuf};

use nakkash_baked::Store;
use nakkash_format::{Diagnostic, Kind, Result, Sha256};

use crate::lock::{ModelLock, Obtain, Pinned};
use crate::{OpenGraph, Tensor, onnx};

/// The command that downloads what the store lacks.
pub const FETCH_COMMAND: &str = "nakkash ml models fetch";

/// The command that takes exported graphs in.
pub const IMPORT_COMMAND: &str = "nakkash ml models import";

/// The model files of a lock on this machine: under `ml/files` of the
/// nakkash cache, each by its hash, with CoreML's compiled models under
/// `ml/compiled` and downloads in progress under `ml/incoming`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelStore {
    root: PathBuf,
    files: Store,
}

/// What became of one file of a folder given to [`ModelStore::import`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imported {
    /// The lock pins it: it is kept, as `name part`.
    Kept {
        /// The file.
        path: PathBuf,
        /// What the lock calls it.
        name: String,
    },
    /// The lock pins nothing with its hash: left out.
    Unknown {
        /// The file.
        path: PathBuf,
    },
}

/// The state of one pinned file in the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    /// What the lock calls it.
    pub name: String,
    /// Its hash.
    pub sha256: Sha256,
    /// Its size in bytes.
    pub size: u64,
    /// Whether the store has it, its hash checked.
    pub present: bool,
}

impl ModelStore {
    /// The store in the nakkash cache at `cache`.
    pub fn new(cache: &Path) -> Self {
        let root = cache.join("ml");
        Self {
            files: Store::new(root.join("files")),
            root,
        }
    }

    /// The files, by hash.
    pub fn files(&self) -> &Store {
        &self.files
    }

    /// The folder downloads go to until they are checked.
    pub fn incoming(&self) -> Store {
        Store::new(self.root.join("incoming"))
    }

    /// Where CoreML keeps its compiled form of the graph with `sha256`,
    /// compiled with `options` by runtime `version` ([D81]): a folder per
    /// combination, so no compile is ever taken for another.
    ///
    /// [D81]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d81-one-inference-runtime-loaded-at-run-time-from-a-locked-file
    pub fn compiled(&self, sha256: &Sha256, options: &str, version: &str) -> PathBuf {
        self.root
            .join("compiled")
            .join(format!("{sha256}-{options}-{version}"))
    }

    /// Graph `graph` of model `model` of `lock`, its file found and hashed
    /// again, ready to open on `threads` threads where the lock places it.
    ///
    /// # Errors
    /// A [`Kind::UnknownName`] diagnostic for a model or graph the lock does
    /// not have, a [`Kind::Missing`] one naming the command that gets the
    /// file when the store lacks it, and a [`Kind::Invalid`] one when it
    /// has changed.
    pub fn graph(
        &self,
        lock: &ModelLock,
        model: &str,
        graph: &str,
        threads: usize,
    ) -> Result<OpenGraph> {
        let locked = lock.model(model)?;
        let found = locked.graphs.get(graph).ok_or_else(|| {
            Diagnostic::unknown("graph", graph, locked.graphs.keys().map(String::as_str))
        })?;
        let path = self.present(&found.sha256, model, graph, &found.obtain)?;
        Ok(OpenGraph {
            model: model.to_owned(),
            graph: graph.to_owned(),
            path,
            sha256: found.sha256,
            placement: found.placement,
            threads,
        })
    }

    /// Constant `name` of model `model`, read from its `.npy` file, which
    /// must hold little-endian 32-bit floats in the locked shape.
    ///
    /// # Errors
    /// As [`ModelStore::graph`], and a [`Kind::Invalid`] diagnostic when the
    /// file is not such an array.
    pub fn constant(&self, lock: &ModelLock, model: &str, name: &str) -> Result<Tensor> {
        let locked = lock.model(model)?;
        let found = locked.constants.get(name).ok_or_else(|| {
            Diagnostic::unknown(
                "constant",
                name,
                locked.constants.keys().map(String::as_str),
            )
        })?;
        let path = self.present(&found.sha256, model, name, &found.obtain)?;
        let bytes = fs::read(&path).map_err(|e| Diagnostic::io("read", &path, &e))?;
        npy_f32(&bytes, &found.shape).map(|values| Tensor::f32(found.shape.clone(), values))
    }

    /// The runtime library for this platform, hashed again.
    ///
    /// # Errors
    /// As [`ModelLock::library_here`], and a [`Kind::Missing`] diagnostic
    /// naming [`FETCH_COMMAND`] when the store lacks it.
    pub fn runtime_library(&self, lock: &ModelLock) -> Result<PathBuf> {
        let library = lock.library_here()?;
        self.files.checked_path(&library.sha256)?.ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!(
                    "the inference runtime {} is not in the model store at {}; run `{FETCH_COMMAND}`",
                    lock.runtime.version,
                    self.files.root().display()
                ),
            )
        })
    }

    /// Takes in every file of `folder` (not its subfolders) that `lock`
    /// pins: graphs once they pass the check of [D84]. Files the lock does
    /// not pin are left out and reported.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the folder or a file cannot be read,
    /// and the check's diagnostic when a pinned graph fails it.
    ///
    /// [D84]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d84-models-are-untrusted-input
    pub fn import(&self, lock: &ModelLock, folder: &Path) -> Result<Vec<Imported>> {
        let entries = fs::read_dir(folder).map_err(|e| Diagnostic::io("read", folder, &e))?;
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        let mut imported = Vec::new();
        for path in paths {
            let (hash, _) = Sha256::of_file(&path)?;
            let Some(pinned) = lock.pinned(&hash) else {
                imported.push(Imported::Unknown { path });
                continue;
            };
            if pinned.graph {
                onnx::check_file(&path)?;
            }
            self.files.put_file(&path, Some(&hash))?;
            imported.push(Imported::Kept {
                path,
                name: describe(&pinned),
            });
        }
        Ok(imported)
    }

    /// The state of every file `lock` pins, the runtime library for this
    /// platform first; each file the store has is hashed again, and each
    /// graph checked again.
    ///
    /// # Errors
    /// A [`Kind::Invalid`] diagnostic when a kept file has changed or a
    /// graph fails the check, and a [`Kind::Io`] one when one cannot be
    /// read.
    pub fn verify(&self, lock: &ModelLock) -> Result<Vec<FileState>> {
        let mut states = Vec::new();
        if let Ok(library) = lock.library_here() {
            states.push(FileState {
                name: format!("runtime {}", lock.runtime.version),
                sha256: library.sha256,
                size: library.size,
                present: self.files.checked_path(&library.sha256)?.is_some(),
            });
        }
        for pinned in lock.files() {
            let path = self.files.checked_path(pinned.sha256)?;
            if let (Some(path), true) = (&path, pinned.graph) {
                onnx::check_file(path)?;
            }
            states.push(FileState {
                name: describe(&pinned),
                sha256: *pinned.sha256,
                size: pinned.size,
                present: path.is_some(),
            });
        }
        Ok(states)
    }

    /// The checked place of a pinned file, or the error that names how to
    /// get it.
    fn present(&self, hash: &Sha256, model: &str, part: &str, obtain: &Obtain) -> Result<PathBuf> {
        self.files.checked_path(hash)?.ok_or_else(|| {
            let how = match obtain {
                Obtain::Download { .. } => format!("run `{FETCH_COMMAND}`"),
                Obtain::Export(_) => {
                    format!("export it, then run `{IMPORT_COMMAND} <folder>` on the folder that holds it")
                }
            };
            Diagnostic::new(
                Kind::Missing,
                format!(
                    "`{model}` {part} ({hash}) is not in the model store at {}; {how}",
                    self.files.root().display()
                ),
            )
        })
    }
}

/// What the lock calls a pinned file: `segment.video image-encoder`.
fn describe(pinned: &Pinned<'_>) -> String {
    format!("{} {}", pinned.name, pinned.part)
}

/// The floats of an `.npy` file of little-endian 32-bit floats in C order
/// and `shape`. Its header must be the one such an array is written with,
/// byte for byte, so nothing in it is interpreted.
fn npy_f32(bytes: &[u8], shape: &[usize]) -> Result<Vec<f32>> {
    let dims = match shape {
        [one] => format!("({one},)"),
        _ => format!(
            "({})",
            shape
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let dict = format!("{{'descr': '<f4', 'fortran_order': False, 'shape': {dims}, }}");
    let bad = || {
        Diagnostic::new(
            Kind::Invalid,
            format!("a constant that is not an array of 32-bit floats of shape {dims}"),
        )
    };
    let header_len = match bytes {
        [0x93, b'N', b'U', b'M', b'P', b'Y', 1, 0, lo, hi, ..] => {
            usize::from(u16::from_le_bytes([*lo, *hi]))
        }
        _ => return Err(bad()),
    };
    let header = bytes.get(10..10 + header_len).ok_or_else(bad)?;
    let text = std::str::from_utf8(header).map_err(|_| bad())?;
    if text.trim_end_matches([' ', '\n']) != dict {
        return Err(bad());
    }
    let data = &bytes[10 + header_len..];
    let count: usize = shape.iter().product();
    if data.len() != count * 4 {
        return Err(bad());
    }
    Ok(data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

#[cfg(test)]
#[path = "../tests/unit/models.rs"]
mod tests;
