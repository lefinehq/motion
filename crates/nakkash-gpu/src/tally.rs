//! What the GPU did over a render, counted as it goes.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use nakkash_raster::Tally;

/// Counts, shared by every thread drawing with one GPU.
#[derive(Default)]
pub(crate) struct Counts {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    accelerated: BTreeMap<String, u64>,
    fell_back: BTreeMap<String, u64>,
    operations: u64,
    uploads: (u64, u64),
    downloads: (u64, u64),
}

impl Counts {
    fn with(&self, f: impl FnOnce(&mut Inner)) {
        f(&mut self.inner.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// An effect run by a kernel.
    pub(crate) fn accelerated(&self, name: &str) {
        self.with(|i| *i.accelerated.entry(name.to_owned()).or_default() += 1);
    }

    /// An effect run on the CPU for want of a kernel.
    pub(crate) fn fell_back(&self, name: &str) {
        self.with(|i| *i.fell_back.entry(name.to_owned()).or_default() += 1);
    }

    /// A painter operation run on the GPU.
    pub(crate) fn operation(&self) {
        self.with(|i| i.operations += 1);
    }

    /// A grid of `bytes` copied to the GPU.
    pub(crate) fn upload(&self, bytes: u64) {
        self.with(|i| i.uploads = (i.uploads.0 + 1, i.uploads.1 + bytes));
    }

    /// A grid of `bytes` copied back to the CPU.
    pub(crate) fn download(&self, bytes: u64) {
        self.with(|i| i.downloads = (i.downloads.0 + 1, i.downloads.1 + bytes));
    }

    /// The counts so far.
    pub(crate) fn tally(&self) -> Tally {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let list = |m: &BTreeMap<String, u64>| m.iter().map(|(k, v)| (k.clone(), *v)).collect();
        Tally {
            accelerated: list(&inner.accelerated),
            fell_back: list(&inner.fell_back),
            operations: inner.operations,
            uploads: inner.uploads,
            downloads: inner.downloads,
        }
    }
}
