//! The pictures a scene's image layers draw (step `picture`): each file
//! read and decoded once per session, when a frame first needs it, however
//! many frames and threads draw it. A file no frame shows is never read.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use motion_format::{Diagnostic, Result};
use motion_raster::Picture;

use crate::stage::stage;

/// Decoded pictures by file, each decoded at most once: the first thread to
/// ask decodes it, and any other asking meanwhile waits for that result.
#[derive(Debug, Default)]
pub(crate) struct Pictures {
    decoded: Mutex<HashMap<PathBuf, Arc<OnceLock<Result<Picture>>>>>,
}

impl Pictures {
    /// The picture in the file at `path`, read and decoded on first use. A
    /// failure is kept too, so a missing file is reported once.
    ///
    /// # Errors
    /// A [`Kind::Io`](motion_format::Kind::Io) diagnostic naming the file
    /// when it cannot be read, or the decoder's when it is not a picture,
    /// with the file in its message.
    pub(crate) fn get(&self, path: &Path) -> Result<Picture> {
        let cell = Arc::clone(
            self.decoded
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(path.to_path_buf())
                .or_default(),
        );
        // Decoded outside the lock, so other files decode at the same time.
        cell.get_or_init(|| stage("picture", || read(path))).clone()
    }
}

/// Reads and decodes the picture file at `path`.
fn read(path: &Path) -> Result<Picture> {
    let bytes = fs::read(path).map_err(|e| {
        Diagnostic::io("read", path, &e)
            .expected("a PNG or JPEG file, relative to the scene file's folder")
    })?;
    Picture::decode(&bytes).map_err(|d| Diagnostic {
        message: format!("`{}` is not a picture: {}", path.display(), d.message),
        ..d
    })
}

#[cfg(test)]
#[path = "../tests/unit/pictures.rs"]
mod tests;
