//! [`Store`]: files kept by their hash.

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use nakkash_format::{Diagnostic, Kind, Result, Sha256, Sha256Hasher};

/// Files kept by their SHA-256 in one folder: baked assets and the files
/// they name ([D85]), or the model files of a lock ([D82]).
///
/// A file is written once, whole, and never changed: it appears under its
/// hash or not at all. Reading checks the hash, so a damaged or replaced
/// file is an error, never other bytes.
///
/// [D82]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d82-a-model-lock-and-a-content-addressed-model-store
/// [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// The store in `root`, which is made when the first file is kept.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Its folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the file with `hash` is kept, whether or not it is there yet.
    /// The place never changes, so a file may be opened from it by tools
    /// that key their own caches by path.
    pub fn path(&self, hash: &Sha256) -> PathBuf {
        self.root.join(hash.to_string())
    }

    /// Whether the store has a file under `hash`, without checking it.
    pub fn has(&self, hash: &Sha256) -> bool {
        self.path(hash).is_file()
    }

    /// Keeps `bytes` and returns their hash. A file the store already has is
    /// left as it is.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be written.
    pub fn put(&self, bytes: &[u8]) -> Result<Sha256> {
        let hash = Sha256::of(bytes);
        if !self.has(&hash) {
            self.write_whole(&hash, |file| file.write_all(bytes))?;
        }
        Ok(hash)
    }

    /// Keeps a copy of the file at `source`, hashed as it is copied, and
    /// returns its hash and size. With `expected`, the copy is kept only if
    /// it has that hash.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when `source` cannot be read or the copy
    /// written, and a [`Kind::Invalid`] one, naming both hashes, when the
    /// copy is not the expected file.
    pub fn put_file(&self, source: &Path, expected: Option<&Sha256>) -> Result<(Sha256, u64)> {
        let reader = File::open(source).map_err(|e| Diagnostic::io("open", source, &e))?;
        self.put_reader(BufReader::new(reader), expected)
            .map_err(|e| e.under(source.display().to_string()))
    }

    /// Keeps what `reader` gives, hashed as it is written, and returns its
    /// hash and size; with `expected`, only if it has that hash. A
    /// download is kept this way, so only the right bytes ever appear.
    ///
    /// # Errors
    /// As [`Store::put_file`].
    pub fn put_reader(
        &self,
        mut reader: impl Read,
        expected: Option<&Sha256>,
    ) -> Result<(Sha256, u64)> {
        let partial = self.partial()?;
        let written = (|| {
            let mut file = File::create(&partial)?;
            let mut hashing = Hashing::new(&mut file);
            let size = io::copy(&mut reader, &mut hashing)?;
            let hash = hashing.finish();
            file.sync_all()?;
            Ok::<_, io::Error>((hash, size))
        })();
        let (hash, size) = match written {
            Ok(done) => done,
            Err(e) => {
                let _ = fs::remove_file(&partial);
                return Err(Diagnostic::io("write", &partial, &e));
            }
        };
        if let Some(expected) = expected.filter(|e| **e != hash) {
            let _ = fs::remove_file(&partial);
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("the file hashes to {hash}, not to the {expected} it should have"),
            ));
        }
        let path = self.path(&hash);
        let kept = match path.is_file() {
            true => fs::remove_file(&partial),
            false => fs::rename(&partial, &path),
        };
        kept.map_err(|e| Diagnostic::io("write", &path, &e))?;
        Ok((hash, size))
    }

    /// The bytes of the file with `hash`, or `None` when the store does not
    /// have it.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be read, or a
    /// [`Kind::Invalid`] one when its bytes no longer have its hash.
    pub fn get(&self, hash: &Sha256) -> Result<Option<Vec<u8>>> {
        let path = self.path(hash);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Diagnostic::io("read", &path, &e)),
        };
        let found = Sha256::of(&bytes);
        match found == *hash {
            true => Ok(Some(bytes)),
            false => Err(changed(&path, &found)),
        }
    }

    /// The place of the file with `hash`, once its bytes are hashed again
    /// and found to be it, or `None` when the store does not have it. For
    /// files too large to hold in memory, which a reader then opens.
    ///
    /// # Errors
    /// As [`Store::get`].
    pub fn checked_path(&self, hash: &Sha256) -> Result<Option<PathBuf>> {
        let path = self.path(hash);
        if !path.is_file() {
            return Ok(None);
        }
        let (found, _) = Sha256::of_file(&path)?;
        match found == *hash {
            true => Ok(Some(path)),
            false => Err(changed(&path, &found)),
        }
    }

    /// Writes a file under `hash` through a file beside it, renamed into
    /// place, so a reader never sees half a file.
    fn write_whole(
        &self,
        hash: &Sha256,
        write: impl FnOnce(&mut File) -> io::Result<()>,
    ) -> Result<()> {
        let partial = self.partial()?;
        let path = self.path(hash);
        let written = File::create(&partial)
            .and_then(|mut file| write(&mut file).and_then(|()| file.sync_all()))
            .and_then(|()| fs::rename(&partial, &path));
        written.map_err(|e| {
            let _ = fs::remove_file(&partial);
            Diagnostic::io("write", &path, &e)
        })
    }

    /// A new name for a file being written, in the store's folder, which is
    /// made if need be.
    fn partial(&self) -> Result<PathBuf> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(&self.root).map_err(|e| Diagnostic::io("create", &self.root, &e))?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Ok(self.root.join(format!(".partial-{}-{n}", process::id())))
    }
}

/// The error for a kept file whose bytes no longer have its hash.
fn changed(path: &Path, found: &Sha256) -> Diagnostic {
    Diagnostic::new(
        Kind::Invalid,
        format!(
            "{} has changed: its bytes hash to {found}; delete it and make it again",
            path.display()
        ),
    )
}

/// A writer that hashes what passes through it.
struct Hashing<'a, W> {
    inner: &'a mut W,
    hasher: Sha256Hasher,
}

impl<'a, W: Write> Hashing<'a, W> {
    fn new(inner: &'a mut W) -> Self {
        Self {
            inner,
            hasher: Sha256Hasher::default(),
        }
    }

    fn finish(self) -> Sha256 {
        self.hasher.finish()
    }
}

impl<W: Write> Write for Hashing<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
#[path = "../tests/unit/store.rs"]
pub(crate) mod tests;
