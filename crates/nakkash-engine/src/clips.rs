//! The clips a scene's video layers play (step `clip`): each file opened
//! once per session, when a frame first shows it, and read frame by frame
//! from then on by [`nakkash_video::Clip`], which keeps its decoder and the
//! frames it decoded last, so frames drawn in order decode once.
//!
//! A video layer with a `cutout` draws each frame with its alpha multiplied
//! by the baked alpha of the same clip frame ([D85]). The matte is found by
//! the digest the scene keeps, and opened only when it answers the layer's
//! request for this clip, by the clip's hash, which is taken once per file.
//!
//! [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers

use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use nakkash_baked::{BAKE_COMMAND, Manifest, Matte, Store};
use nakkash_format::key::CUTOUT;
use nakkash_format::{Cutout, Diagnostic, Kind, Result, Sha256};
use nakkash_raster::{Image, Picture};
use nakkash_video::Clip;

use crate::stage::stage;

/// A value made once by the first thread to ask, which others asking
/// meanwhile wait for; a failure is kept, so it is reported once.
type Once<T> = Arc<OnceLock<Result<T>>>;

/// Which matte a layer reads: its digest, its request as JSON text, and
/// the clip it cuts.
type MatteKey = (Sha256, String, PathBuf);

/// Open clips by file, each opened at most once, with their hashes and the
/// mattes cutting them, each read at most once.
#[derive(Default)]
pub(crate) struct Clips {
    opened: Mutex<HashMap<PathBuf, Once<Arc<Clip>>>>,
    hashes: Mutex<HashMap<PathBuf, Once<Sha256>>>,
    mattes: Mutex<HashMap<MatteKey, Once<Arc<Manifest<Matte>>>>>,
}

impl fmt::Debug for Clips {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let opened = self.opened.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("Clips")
            .field("opened", &opened.len())
            .finish()
    }
}

/// The cell for `key` in `map`, made empty if it is new. The map's lock is
/// held only to find the cell, so other keys are made at the same time.
fn cell<K: Eq + Hash, T>(map: &Mutex<HashMap<K, Once<T>>>, key: K) -> Once<T> {
    Arc::clone(
        map.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key)
            .or_default(),
    )
}

impl Clips {
    /// The frame of the clip at `path` showing `seconds` into it, as a
    /// picture, or `None` before it starts and from its end on. With a
    /// `cutout`, its alpha is multiplied by the baked matte's, read from
    /// the store `store` gives, which is asked for only then.
    ///
    /// # Errors
    /// An I/O diagnostic naming the file when it cannot be read, or the
    /// clip's when it is not a clip, is in a codec there is no decoder for,
    /// or has a frame that does not decode. For a cutout, a diagnostic at
    /// `cutout`: [`Kind::Missing`] naming the bake command when it has no
    /// bake or the store lacks it, [`Kind::Conflict`] when the bake was made
    /// for another request or clip, and [`Kind::Invalid`] when the matte's
    /// frames do not fit the clip's.
    pub(crate) fn frame(
        &self,
        path: &Path,
        seconds: f64,
        cutout: Option<&Cutout>,
        store: &dyn Fn() -> Result<Store>,
    ) -> Result<Option<Picture>> {
        let clip = cell(&self.opened, path.to_path_buf())
            .get_or_init(|| stage("clip", || Clip::open(path).map(Arc::new)))
            .clone()?;
        let Some(index) = clip.shown_at(seconds) else {
            return Ok(None);
        };
        let frame = clip.frame(index)?;
        let mut pixels = frame.pixels.clone();
        if let Some(cutout) = cutout {
            self.cut(path, &clip, index, cutout, store, &mut pixels)
                .map_err(|e| e.at(CUTOUT))?;
        }
        let image = Image::new(frame.width, frame.height, pixels)?;
        Picture::from_image(&image).map(Some)
    }

    /// Multiplies the alpha of `pixels`, frame `index` of `clip` in straight
    /// RGBA, by the baked alpha of the same frame.
    fn cut(
        &self,
        path: &Path,
        clip: &Clip,
        index: usize,
        cutout: &Cutout,
        store: &dyn Fn() -> Result<Store>,
        pixels: &mut [u8],
    ) -> Result<()> {
        let digest = cutout.baked.ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!("the cutout has not been baked; run `{BAKE_COMMAND}` on the scene"),
            )
            .expected(r#"a `baked` digest, which the bake writes: {"cutout":{…,"baked":"3f9a…"}}"#)
        })?;
        let store = store()?;
        let hash = cell(&self.hashes, path.to_path_buf())
            .get_or_init(|| stage("hash", || Sha256::of_file(path).map(|(hash, _)| hash)))
            .clone()?;
        let request = cutout.request();
        let key = (digest, request.to_string(), path.to_path_buf());
        let matte = cell(&self.mattes, key)
            .get_or_init(|| Manifest::<Matte>::open(&store, &digest, &request, &hash).map(Arc::new))
            .clone()?;
        if matte.frames() != clip.frames() {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "the baked matte {digest} has {} frames and the clip {}; run `{BAKE_COMMAND}` on the scene",
                    matte.frames(),
                    clip.frames()
                ),
            ));
        }
        let alpha = matte
            .alpha(&store, index)?
            .ok_or_else(|| Diagnostic::new(Kind::Invalid, "a matte frame past its end"))?;
        cut_alpha(pixels, &alpha.values);
        Ok(())
    }
}

/// Multiplies each pixel's 8-bit alpha in `pixels`, straight RGBA, by its
/// 16-bit value in `matte`, rounded to the nearest 8-bit level: an opaque
/// matte keeps the pixel, a clear one clears it.
pub(crate) fn cut_alpha(pixels: &mut [u8], matte: &[u16]) {
    for (pixel, &m) in pixels.chunks_exact_mut(4).zip(matte) {
        let product = u32::from(pixel[3]) * u32::from(m);
        pixel[3] = ((product + 32767) / 65535) as u8;
    }
}

#[cfg(test)]
#[path = "../tests/unit/clips.rs"]
mod tests;
