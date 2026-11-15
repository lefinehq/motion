//! The clips a scene's video layers play (step `clip`): each file opened
//! once per session, when a frame first shows it, and read frame by frame
//! from then on by [`motion_video::Clip`], which keeps its decoder and the
//! frames it decoded last, so frames drawn in order decode once.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use motion_format::Result;
use motion_raster::{Image, Picture};
use motion_video::Clip;

use crate::stage::stage;

/// One clip's opening: set once by the first thread to ask, with the clip
/// or why it could not be opened.
type Opening = Arc<OnceLock<Result<Arc<Clip>>>>;

/// Open clips by file, each opened at most once: the first thread to ask
/// opens it, and any other asking meanwhile waits for that result.
#[derive(Default)]
pub(crate) struct Clips {
    opened: Mutex<HashMap<PathBuf, Opening>>,
}

impl fmt::Debug for Clips {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let opened = self.opened.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("Clips")
            .field("opened", &opened.len())
            .finish()
    }
}

impl Clips {
    /// The frame of the clip at `path` showing `seconds` into it, as a
    /// picture, or `None` before it starts and from its end on. A failure
    /// to open is kept, so a missing file is reported once.
    ///
    /// # Errors
    /// An I/O diagnostic naming the file when it cannot be read, or the
    /// clip's when it is not a clip, is in a codec there is no decoder for,
    /// or has a frame that does not decode.
    pub(crate) fn frame(&self, path: &Path, seconds: f64) -> Result<Option<Picture>> {
        let cell = Arc::clone(
            self.opened
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(path.to_path_buf())
                .or_default(),
        );
        // Opened outside the lock, so other files open at the same time.
        let clip = cell
            .get_or_init(|| stage("clip", || Clip::open(path).map(Arc::new)))
            .clone()?;
        let Some(frame) = clip.frame_at(seconds)? else {
            return Ok(None);
        };
        let image = Image::new(frame.width, frame.height, frame.pixels.clone())?;
        Picture::from_image(&image).map(Some)
    }
}
