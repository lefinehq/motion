//! [`Cache`]: font files on this machine, kept by their hash.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use nakkash_format::deprecated::TARGET as DEPRECATED;
use nakkash_format::{Diagnostic, Kind, Result};
use tracing::warn;

use crate::{LockedFont, Sha256};

/// The variable that moves the cache.
pub const CACHE_VARIABLE: &str = "NAKKASH_CACHE";

/// [`CACHE_VARIABLE`] under the project's old name: read when the new one
/// is not set, with a deprecation note ([D73]).
///
/// [D73]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d73-the-project-is-named-nakkash
pub const OLD_CACHE_VARIABLE: &str = "MOTION_CACHE";

/// The cache's folder name in the platform's cache folder, and its name
/// before the project was renamed ([D73]).
///
/// [D73]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d73-the-project-is-named-nakkash
const FOLDER: &str = "nakkash";
/// The folder [`FOLDER`] replaced, still read.
const OLD_FOLDER: &str = "motion";

/// Font files kept by their hash, under `fonts/` of a cache folder: the
/// only place a render takes a locked font from ([D39]). A file is written
/// once, whole, and never changed; reading checks its hash, so a damaged or
/// replaced file is an error, not a different font.
///
/// [D39]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    root: PathBuf,
    /// The cache under the project's old name, read for files `root` lacks,
    /// each of which then moves into `root` ([D73]).
    ///
    /// [D73]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d73-the-project-is-named-nakkash
    old: Option<PathBuf>,
}

impl Cache {
    /// The cache in `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            old: None,
        }
    }

    /// The cache in `root` that also reads the files `old` has and `root`
    /// lacks, copying each one it reads into `root`.
    pub fn with_old(root: impl Into<PathBuf>, old: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            old: Some(old.into()),
        }
    }

    /// This user's cache: `$NAKKASH_CACHE` (or the old `$MOTION_CACHE`),
    /// else the platform's cache folder (`~/Library/Caches/nakkash`,
    /// `%LOCALAPPDATA%\nakkash`, or `$XDG_CACHE_HOME/nakkash`, by default
    /// `~/.cache/nakkash`). When the platform's folder still has the cache
    /// under its old name, `motion`, files are also read from there ([D73]).
    ///
    /// # Errors
    /// A [`Kind::Missing`] diagnostic when the environment names no home.
    ///
    /// [D73]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d73-the-project-is-named-nakkash
    pub fn user() -> Result<Self> {
        let var = |name| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if let Some(root) = var(CACHE_VARIABLE) {
            return Ok(Self::new(root));
        }
        if let Some(root) = var(OLD_CACHE_VARIABLE) {
            warn!(
                target: DEPRECATED,
                "{OLD_CACHE_VARIABLE} is the old name of {CACHE_VARIABLE}; set {CACHE_VARIABLE} instead (D73)"
            );
            return Ok(Self::new(root));
        }
        let platform = if cfg!(target_os = "macos") {
            var("HOME").map(|home| home.join("Library/Caches"))
        } else if cfg!(windows) {
            var("LOCALAPPDATA")
        } else {
            var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|home| home.join(".cache")))
        };
        platform
            .map(|dir| match dir.join(OLD_FOLDER) {
                old if old.is_dir() => Self::with_old(dir.join(FOLDER), old),
                _ => Self::new(dir.join(FOLDER)),
            })
            .ok_or_else(|| {
                Diagnostic::new(
                    Kind::Missing,
                    format!("no folder for the font cache: set `{CACHE_VARIABLE}`"),
                )
            })
    }

    /// The cache's folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the file with `hash` is kept.
    pub fn font_path(&self, hash: &Sha256) -> PathBuf {
        self.root.join("fonts").join(hash.to_string())
    }

    /// The bytes of the file with `hash`, or none when the cache does not
    /// have it.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be read, or a
    /// [`Kind::Invalid`] one when its bytes no longer have its hash.
    pub fn get(&self, hash: &Sha256) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = read_checked(&self.font_path(hash), hash)? {
            return Ok(Some(bytes));
        }
        let Some(old) = &self.old else {
            return Ok(None);
        };
        let path = old.join("fonts").join(hash.to_string());
        let Some(bytes) = read_checked(&path, hash)? else {
            return Ok(None);
        };
        warn!(
            target: DEPRECATED,
            "font file {} is copied from the cache's old folder into {} (D73)",
            path.display(),
            self.root.display()
        );
        self.put(&bytes)?;
        Ok(Some(bytes))
    }

    /// The bytes of `font`'s file.
    ///
    /// # Errors
    /// As [`Cache::get`], and a [`Kind::Missing`] diagnostic, naming the
    /// command that fetches it, when the cache does not have it.
    pub fn load(&self, font: &LockedFont) -> Result<Vec<u8>> {
        self.get(&font.sha256)?.ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!(
                    "font `{}` ({} {}) is not in the font cache at {}; run `nakkash fonts fetch` on the scene",
                    font.postscript,
                    font.family,
                    font.version,
                    self.root.display()
                ),
            )
        })
    }

    /// Keeps `bytes` and returns their hash. A file the cache already has is
    /// left as it is; a new one appears whole or not at all.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be written.
    pub fn put(&self, bytes: &[u8]) -> Result<Sha256> {
        let hash = Sha256::of(bytes);
        let path = self.font_path(&hash);
        if path.is_file() {
            return Ok(hash);
        }
        write_whole(&path, bytes)?;
        Ok(hash)
    }
}

/// The bytes of the file at `path`, which must hash to `hash`, or none when
/// there is no such file.
fn read_checked(path: &Path, hash: &Sha256) -> Result<Option<Vec<u8>>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Diagnostic::io("read", path, &e)),
    };
    let found = Sha256::of(&bytes);
    if found != *hash {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "the font cache's {} has changed: its bytes hash to {found}; delete it and fetch again",
                path.display()
            ),
        ));
    }
    Ok(Some(bytes))
}

/// Writes `bytes` to `path` through a file beside it, renamed into place,
/// so a reader never sees half a file.
pub(crate) fn write_whole(path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().unwrap_or(Path::new(""));
    fs::create_dir_all(dir).map_err(|e| Diagnostic::io("create", dir, &e))?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let partial = dir.join(format!(".partial-{}-{n}", process::id()));
    let written = fs::File::create(&partial)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::rename(&partial, path));
    written.map_err(|e| {
        let _ = fs::remove_file(&partial);
        Diagnostic::io("write", path, &e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Licence, Origin};

    fn scratch(name: &str) -> Cache {
        let root = std::env::temp_dir().join(format!("nakkash-fonts-{name}-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        Cache::new(root)
    }

    fn locked(bytes: &[u8]) -> LockedFont {
        LockedFont {
            postscript: "Test-Regular".into(),
            family: "Test".into(),
            version: "Version 1.000".into(),
            licence: Licence::Ofl,
            source: Origin {
                repository: "owner/test".into(),
                reference: "v1".into(),
                path: "Test-Regular.ttf".into(),
            },
            sha256: Sha256::of(bytes),
            size: bytes.len() as u64,
        }
    }

    #[test]
    fn gives_back_what_it_keeps() {
        let cache = scratch("keep");
        let hash = cache.put(b"font bytes").unwrap();
        assert_eq!(hash, Sha256::of(b"font bytes"));
        assert_eq!(cache.put(b"font bytes").unwrap(), hash);
        assert_eq!(cache.load(&locked(b"font bytes")).unwrap(), b"font bytes");
        fs::remove_dir_all(cache.root()).unwrap();
    }

    #[test]
    fn names_the_command_for_a_missing_font() {
        let cache = scratch("missing");
        let error = cache.load(&locked(b"absent")).unwrap_err();
        assert_eq!(error.kind, Kind::Missing);
        assert!(
            error.message.contains("nakkash fonts fetch"),
            "{}",
            error.message
        );
    }

    #[test]
    fn refuses_a_changed_file() {
        let cache = scratch("changed");
        let hash = cache.put(b"original").unwrap();
        fs::write(cache.font_path(&hash), b"replaced").unwrap();
        let error = cache.get(&hash).unwrap_err();
        assert_eq!(error.kind, Kind::Invalid);
        assert!(error.message.contains("has changed"), "{}", error.message);
        fs::remove_dir_all(cache.root()).unwrap();
    }

    #[test]
    fn reads_the_old_folder_and_copies_what_it_reads() {
        let old = scratch("old-folder");
        let hash = old.put(b"kept before the rename").unwrap();
        let new = scratch("new-folder");
        let cache = Cache::with_old(new.root(), old.root());
        let font = locked(b"kept before the rename");
        assert_eq!(cache.load(&font).unwrap(), b"kept before the rename");
        assert!(
            cache.font_path(&hash).is_file(),
            "copied into the new folder"
        );
        fs::remove_dir_all(old.root()).unwrap();
        assert_eq!(cache.load(&font).unwrap(), b"kept before the rename");
        assert!(cache.get(&Sha256::of(b"in neither")).unwrap().is_none());
        fs::remove_dir_all(new.root()).unwrap();
    }
}
