//! [`Sha256`]: the content address of a file, as locks pin it and caches
//! keep it: a font file ([D39]), a model file or a baked asset ([D82],
//! [D85]).
//!
//! [D39]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render
//! [D82]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d82-a-model-lock-and-a-content-addressed-model-store
//! [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::Digest;

use crate::{Diagnostic, Kind, Result};

/// How many bytes [`Sha256::of_reader`] reads at a time.
const CHUNK: usize = 1 << 20;

/// The SHA-256 of a file's bytes: what a lock pins and where a cache keeps
/// it. Written as 64 lowercase hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    /// The hash of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha2::Sha256::digest(bytes).into())
    }

    /// The hash of everything `reader` gives, read a megabyte at a time, so
    /// a file of any size is hashed in constant memory.
    ///
    /// # Errors
    /// The reader's error.
    pub fn of_reader(mut reader: impl Read) -> io::Result<Self> {
        let mut hasher = Sha256Hasher::default();
        let mut chunk = vec![0; CHUNK];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => return Ok(hasher.finish()),
                Ok(n) => hasher.update(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// The hash of the file at `path` and its size in bytes, read without
    /// holding the file in memory.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic naming the file when it cannot be read.
    pub fn of_file(path: &Path) -> Result<(Self, u64)> {
        let file = File::open(path).map_err(|e| Diagnostic::io("open", path, &e))?;
        let mut counted = Counted { inner: file, n: 0 };
        let hash = Self::of_reader(&mut counted).map_err(|e| Diagnostic::io("read", path, &e))?;
        Ok((hash, counted.n))
    }

    /// The hash with these 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Its 32 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A [`Sha256`] taken in pieces: fed bytes as they pass, finished once.
#[derive(Clone, Default)]
pub struct Sha256Hasher(sha2::Sha256);

impl Sha256Hasher {
    /// Feeds it `bytes`, the next ones of what it hashes.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The hash of everything it was fed.
    pub fn finish(self) -> Sha256 {
        Sha256(self.0.finalize().into())
    }
}

impl fmt::Debug for Sha256Hasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sha256Hasher")
    }
}

/// A reader that counts the bytes it gives.
struct Counted<R> {
    inner: R,
    n: u64,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.n += n as u64;
        Ok(n)
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256({self})")
    }
}

impl FromStr for Sha256 {
    type Err = Diagnostic;

    fn from_str(text: &str) -> Result<Self> {
        let invalid = || {
            Diagnostic::new(
                Kind::Invalid,
                format!("`{text}` is not a SHA-256: it takes 64 hexadecimal digits"),
            )
        };
        if text.len() != 64 || !text.is_ascii() {
            return Err(invalid());
        }
        let mut bytes = [0; 32];
        for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
            let pair = std::str::from_utf8(pair).map_err(|_| invalid())?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| invalid())?;
        }
        Ok(Self(bytes))
    }
}

impl Serialize for Sha256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse()
            .map_err(|e: Diagnostic| serde::de::Error::custom(e.message))
    }
}

#[cfg(test)]
#[path = "../tests/unit/sha256.rs"]
mod tests;
