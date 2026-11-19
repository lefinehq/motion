//! Frames a render already drew, kept by their identity ([D90]): a frame
//! whose display list has the identity of one drawn before in the same
//! render is that frame's image, copied, not drawn again. Held frames are
//! a quarter to nearly all the frames of many templates.
//!
//! The images kept are the most recent identities, as many as fit in an
//! eighth of the memory budget and at most two per thread; a thread that
//! asks for an identity another is drawing waits for it instead of drawing
//! it too. Over the budget nothing new is kept.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use nakkash_format::Result;
use nakkash_raster::Image;

/// Frames copied from one of the same identity, over the process.
static COPIED: AtomicU64 = AtomicU64::new(0);

/// How many frames were copied from a frame of the same identity instead
/// of drawn, over the process.
pub fn frames_copied() -> u64 {
    COPIED.load(Ordering::Relaxed)
}

/// What one identity drew, once it is drawn.
type Slot = Arc<OnceLock<Result<Image>>>;

/// The images of one render's frames, by identity.
pub(crate) struct Held {
    slots: Mutex<(HashMap<u128, Slot>, VecDeque<u128>)>,
    /// How many images are kept at most.
    most: usize,
}

impl Held {
    /// Room for frames of `bytes` each, drawn on `threads` threads.
    pub(crate) fn new(bytes: u64, threads: usize) -> Held {
        let room = nakkash_raster::memory::stats().effective / 8;
        let fit = usize::try_from(room / bytes.max(1)).unwrap_or(usize::MAX);
        Held {
            slots: Mutex::default(),
            most: fit.min(2 * threads.max(1)),
        }
    }

    /// The image of the frame of identity `id`: the one drawn for it in
    /// this render, or `draw`'s.
    ///
    /// # Errors
    /// The one `draw` returns, for every frame of its identity.
    pub(crate) fn image(&self, id: u128, draw: impl FnOnce() -> Result<Image>) -> Result<Image> {
        if self.most == 0 {
            return draw();
        }
        let slot = {
            let mut slots = self.slots();
            let (map, order) = &mut *slots;
            match map.get(&id) {
                Some(slot) => Arc::clone(slot),
                None if nakkash_raster::memory::over() => return draw(),
                None => {
                    let slot: Slot = Arc::default();
                    map.insert(id, Arc::clone(&slot));
                    order.push_back(id);
                    while order.len() > self.most {
                        if let Some(old) = order.pop_front() {
                            map.remove(&old);
                        }
                    }
                    slot
                }
            }
        };
        let mut draw = Some(draw);
        let image = slot
            .get_or_init(|| (draw.take().expect("drawn once"))())
            .clone();
        if draw.is_some() {
            COPIED.fetch_add(1, Ordering::Relaxed);
        }
        // Verify mode: a frame copied is drawn again and compared.
        if let (Some(draw), Ok(kept)) = (draw, &image)
            && nakkash_raster::verify_hit()
            && draw()? != *kept
        {
            return Err(nakkash_format::Diagnostic::new(
                nakkash_format::Kind::Invalid,
                format!("a frame copied by its identity {id:032x} differs from its drawing"),
            )
            .expected("frames of one identity that draw the same (D90)"));
        }
        image
    }

    fn slots(&self) -> MutexGuard<'_, (HashMap<u128, Slot>, VecDeque<u128>)> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "../tests/unit/held.rs"]
mod tests;
