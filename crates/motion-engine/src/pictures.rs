//! The pictures a scene's image layers draw (step `picture`): each file
//! read and decoded once per session, when a frame first needs it, however
//! many frames and threads draw it. A file no frame shows is never read.
//!
//! A file is a raster picture (PNG, JPEG) or vector footage (PDF, and the
//! `.ai` files that are PDF inside, [D70]), told apart by how it starts.
//! Vector footage is read into a [`Drawing`] once; the pictures drawn of it
//! at each size are kept with it.
//!
//! [D70]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d70-vector-footage-is-drawn-from-its-outlines

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use motion_format::{Diagnostic, Result};
use motion_raster::Picture;
use motion_vector::Drawing;

use crate::stage::stage;

/// What an image layer's file holds.
#[derive(Debug, Clone)]
pub(crate) enum Still {
    /// Pixels.
    Picture(Picture),
    /// Vector footage, drawn at the size each frame asks for.
    Drawing(Arc<Drawing>),
}

/// Decoded pictures by file, each decoded at most once: the first thread to
/// ask decodes it, and any other asking meanwhile waits for that result.
#[derive(Debug, Default)]
pub(crate) struct Pictures {
    decoded: Mutex<HashMap<PathBuf, Arc<OnceLock<Result<Still>>>>>,
}

impl Pictures {
    /// The picture in the file at `path`, read and decoded on first use. A
    /// failure is kept too, so a missing file is reported once.
    ///
    /// # Errors
    /// A [`Kind::Io`](motion_format::Kind::Io) diagnostic naming the file
    /// when it cannot be read, or the decoder's when it is not a picture,
    /// with the file in its message.
    pub(crate) fn get(&self, path: &Path) -> Result<Still> {
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

/// Reads and decodes the picture file at `path`: vector footage when it
/// starts as a vector format does, else a raster picture.
fn read(path: &Path) -> Result<Still> {
    let bytes = fs::read(path).map_err(|e| {
        Diagnostic::io("read", path, &e)
            .expected("a PNG, JPEG or PDF file, relative to the scene file's folder")
    })?;
    let still = match motion_vector::recognise(&bytes) {
        Some(format) => (format.read)(&bytes).map(|drawing| Still::Drawing(Arc::new(drawing))),
        None => Picture::decode(&bytes).map(Still::Picture).map_err(|d| {
            match d.message == "a file of no picture format" {
                // Vector formats are pictures too.
                true => d.expected("a PNG, JPEG or PDF file"),
                false => d,
            }
        }),
    };
    still.map_err(|d| Diagnostic {
        message: format!("`{}` is not a picture: {}", path.display(), d.message),
        ..d
    })
}

#[cfg(test)]
#[path = "../tests/unit/pictures.rs"]
mod tests;
