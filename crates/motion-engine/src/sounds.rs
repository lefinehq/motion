//! The sound files a scene's audio layers play (step `sound`): each read,
//! decoded and laid out for the mix once per session, when the mix first
//! needs it, and shared from then on.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use motion_audio::Sound;
use motion_format::Result;

use crate::stage::stage;

/// One file's reading: set once by the first thread to ask, with the sound
/// or why it could not be read.
type Reading = Arc<OnceLock<Result<Arc<Sound>>>>;

/// Sounds by file, each read at most once.
#[derive(Default)]
pub(crate) struct Sounds {
    read: Mutex<HashMap<PathBuf, Reading>>,
}

impl fmt::Debug for Sounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("Sounds").field("read", &read.len()).finish()
    }
}

impl Sounds {
    /// The sound of the file at `path`, stereo at the mix rate. A failure
    /// is kept, so a missing file is reported once.
    ///
    /// # Errors
    /// An I/O diagnostic naming the file when it cannot be read, or the
    /// decoder's when it is in no format there is a decoder for.
    pub(crate) fn get(&self, path: &Path) -> Result<Arc<Sound>> {
        let cell = Arc::clone(
            self.read
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(path.to_path_buf())
                .or_default(),
        );
        // Read outside the lock, so other files read at the same time.
        cell.get_or_init(|| {
            stage("sound", || {
                motion_audio::open(path).map(|sound| Arc::new(sound.for_mix()))
            })
        })
        .clone()
    }
}
