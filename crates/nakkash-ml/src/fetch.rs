//! [`Fetcher`]: downloads what a lock pins and the store lacks ([D82]): the
//! runtime library for this platform and the models released as files.
//! The only code of the crate that reaches the network, behind its `fetch`
//! feature, which only the CLI turns on.
//!
//! Every download goes to `ml/incoming` first, is kept there only with the
//! hash the lock gives, and enters the store only once it passes what a
//! file of its kind must: a graph the check of [D84], the runtime's package
//! the registry's own SHA-512.
//!
//! [D82]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d82-a-model-lock-and-a-content-addressed-model-store
//! [D84]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d84-models-are-untrusted-input

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use sha2::Digest;
use ureq::Agent;

use nakkash_baked::Store;
use nakkash_format::{Diagnostic, Kind, Result, Sha256};

use crate::lock::{Member, ModelLock, Obtain};
use crate::{ModelStore, onnx};

/// What a fetch did with one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// Downloaded and kept.
    Kept {
        /// What the lock calls it.
        name: String,
        /// Its size in bytes.
        size: u64,
    },
    /// The store already had it.
    Present {
        /// What the lock calls it.
        name: String,
    },
    /// We export it; it is taken in by [`IMPORT_COMMAND`](crate::IMPORT_COMMAND), not fetched.
    Exported {
        /// What the lock calls it.
        name: String,
    },
}

/// Downloads into a [`ModelStore`].
pub struct Fetcher<'a> {
    agent: Agent,
    store: &'a ModelStore,
    progress: Box<dyn Fn(&str) + 'a>,
}

impl std::fmt::Debug for Fetcher<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fetcher")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl<'a> Fetcher<'a> {
    /// A fetcher into `store`, telling `progress` what it starts.
    pub fn new(store: &'a ModelStore, progress: impl Fn(&str) + 'a) -> Self {
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .user_agent(concat!("nakkash-ml/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: config.into(),
            store,
            progress: Box::new(progress),
        }
    }

    /// The runtime library for this platform: its package downloaded and
    /// checked against both its hashes, the library taken out of it.
    ///
    /// # Errors
    /// As [`ModelLock::library_here`]; a [`Kind::Io`] diagnostic when the
    /// download fails; a [`Kind::Invalid`] one when the package or the
    /// library is not what the lock pins.
    pub fn runtime(&self, lock: &ModelLock) -> Result<Fetched> {
        let library = lock.library_here()?;
        let name = format!("runtime {} ({})", lock.runtime.version, library.path);
        if self.store.files().has(&library.sha256) {
            return Ok(Fetched::Present { name });
        }
        let package = &lock.runtime.package;
        let incoming = self.store.incoming();
        (self.progress)(&format!(
            "downloading {} ({} bytes)",
            package.url, package.size
        ));
        self.download(&package.url, package.size, &package.sha256, &incoming)?;
        let path = incoming.path(&package.sha256);
        let taken = check_sha512(&path, &package.sha512)
            .and_then(|()| take_member(&path, library, self.store.files()));
        let _ = fs::remove_file(&path);
        taken?;
        Ok(Fetched::Kept {
            name,
            size: library.size,
        })
    }

    /// Every model file `lock` releases as a download, of the models in
    /// `only` or of all; exported files are listed, not fetched.
    ///
    /// # Errors
    /// As [`Fetcher::runtime`], and the check's diagnostic for a graph that
    /// fails it, which is then not kept.
    pub fn models(&self, lock: &ModelLock, only: &[String]) -> Result<Vec<Fetched>> {
        for key in only {
            lock.model(key)?;
        }
        let mut done = Vec::new();
        for pinned in lock.files() {
            if !only.is_empty() && !only.iter().any(|k| k == pinned.name) {
                continue;
            }
            let name = format!("{} {}", pinned.name, pinned.part);
            if self.store.files().has(pinned.sha256) {
                done.push(Fetched::Present { name });
                continue;
            }
            let Some(Obtain::Download { url }) = pinned.obtain else {
                done.push(Fetched::Exported { name });
                continue;
            };
            (self.progress)(&format!(
                "downloading {name}: {url} ({} bytes)",
                pinned.size
            ));
            let incoming = self.store.incoming();
            self.download(url, pinned.size, pinned.sha256, &incoming)?;
            let path = incoming.path(pinned.sha256);
            if pinned.graph
                && let Err(e) = onnx::check_file(&path)
            {
                let _ = fs::remove_file(&path);
                return Err(e);
            }
            let kept = self.store.files().path(pinned.sha256);
            fs::create_dir_all(self.store.files().root())
                .and_then(|()| fs::rename(&path, &kept))
                .map_err(|e| Diagnostic::io("write", &kept, &e))?;
            done.push(Fetched::Kept {
                name,
                size: pinned.size,
            });
        }
        Ok(done)
    }

    /// Downloads `url`, `size` bytes, into `store`, kept only if it hashes
    /// to `sha256`.
    fn download(&self, url: &str, size: u64, sha256: &Sha256, store: &Store) -> Result<()> {
        let failed =
            |why: String| Diagnostic::new(Kind::Io, format!("could not download {url}: {why}"));
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| failed(e.to_string()))?;
        if response.status() != 200 {
            return Err(failed(format!("the server answered {}", response.status())));
        }
        let reader = response
            .into_body()
            .into_with_config()
            .limit(size + 1)
            .reader();
        store
            .put_reader(reader, Some(sha256))
            .map(drop)
            .map_err(|e| e.under(url.to_owned()))
    }
}

/// Checks that the file at `path` has the SHA-512 `expected`, in base64 as
/// a package registry publishes it.
///
/// # Errors
/// A [`Kind::Io`] diagnostic when it cannot be read, and a
/// [`Kind::Invalid`] one when its hash is another.
pub fn check_sha512(path: &Path, expected: &str) -> Result<()> {
    let mut file = BufReader::new(File::open(path).map_err(|e| Diagnostic::io("open", path, &e))?);
    let mut hasher = sha2::Sha512::new();
    let mut chunk = vec![0; 1 << 20];
    loop {
        let n = file
            .read(&mut chunk)
            .map_err(|e| Diagnostic::io("read", path, &e))?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk[..n]);
    }
    let found = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
    match found == expected {
        true => Ok(()),
        false => Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "{} has the SHA-512 {found}, not the {expected} its registry publishes",
                path.display()
            ),
        )),
    }
}

/// Takes `member` out of the zip archive at `package` into `store`, kept
/// only with its locked hash.
///
/// # Errors
/// A [`Kind::Invalid`] diagnostic when the archive does not read or has no
/// such member, or the member is not the locked file; a [`Kind::Io`] one
/// when it cannot be written.
pub fn take_member(package: &Path, member: &Member, store: &Store) -> Result<()> {
    let file = File::open(package).map_err(|e| Diagnostic::io("open", package, &e))?;
    let bad = |why: String| Diagnostic::new(Kind::Invalid, format!("{}: {why}", package.display()));
    let mut archive = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| bad(e.to_string()))?;
    let entry = archive
        .by_name(&member.path)
        .map_err(|_| bad(format!("no {} in the package", member.path)))?;
    if entry.size() != member.size {
        return Err(bad(format!(
            "{} is {} bytes, not {}",
            member.path,
            entry.size(),
            member.size
        )));
    }
    store.put_reader(entry, Some(&member.sha256)).map(drop)
}

#[cfg(test)]
#[path = "../tests/unit/fetch.rs"]
mod tests;
