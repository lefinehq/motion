//! What blurs, glows and shadows made, kept by what they read ([D93]).
//!
//! A glow, Gaussian Blur, Fast Box Blur or drop shadow is a function of its
//! layer's pixels, its values and the transform of the space it reads, and
//! of nothing else. When the same three come again in a render, in the same
//! frame or a later one, the pixels it made then are what it makes now, so
//! they are copied instead of being worked out again.
//!
//! - **Keys.** A 128-bit hash ([`Hasher`], SipHash 1-3) of the effect's name
//!   and values, numbers by their bits, the transform's bits, the grid's
//!   size and every bit of every pixel. Pixels are hashed in runs of a fixed
//!   length on every thread and the runs' hashes then in order, so a key is
//!   the same however the work is split.
//! - **Room.** Results count as drawings kept for later uses, within a
//!   sixteenth of the budget in force, so drawings a frame reads again and
//!   frames drawn side by side keep the rest. A result larger than half of
//!   that is neither hashed nor kept; the least recently used go first, and
//!   memory pressure, which lowers the budget, lets them go.
//! - **Verify.** Under `NAKKASH_VERIFY_HITS` a hit is worked out again and
//!   compared bit for bit, as [D90]'s kept drawings are; a difference means
//!   a key left out something the effect reads, and stops the render.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget
//! [D93]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d93-glow-and-blurs-do-less-of-the-same-work

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rayon::prelude::*;

use crate::effect::ParamValue;
use crate::identity::{Hasher, Id};
use crate::memory;
use crate::pixels::{Pixel, Pixels};

/// The effects whose result is kept: each reads only its pixels, its
/// values and its space's transform.
pub(crate) const KEPT: &[&str] = &["glow", "gaussian-blur", "box-blur", "drop-shadow"];

/// How many pixels one run of the pixels' hash covers.
const RUN: usize = 1 << 14;

/// One kept result, and when it was last used.
struct Kept {
    pixels: Arc<Pixels>,
    used: u64,
}

#[derive(Default)]
struct Store {
    results: HashMap<Id, Kept>,
    bytes: u64,
    clock: u64,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);
static REUSED: AtomicU64 = AtomicU64::new(0);

fn store() -> MutexGuard<'static, Option<Store>> {
    STORE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How many effects, over the process, gave a result kept by what they
/// read instead of working it out.
pub fn effects_reused() -> u64 {
    REUSED.load(Relaxed)
}

/// Lets every kept result go.
#[cfg(test)]
pub(crate) fn let_go() {
    if let Some(store) = store().take() {
        memory::keep(store.bytes, false);
    }
}

/// The key of `name` with `values` run on `pixels` in a space of
/// `transform`.
pub(crate) fn key(
    name: &str,
    values: &[ParamValue],
    transform: vello_cpu::kurbo::Affine,
    pixels: &Pixels,
) -> Id {
    let runs: Vec<u128> = pixels
        .data()
        .par_chunks(RUN)
        .map(|run| {
            let mut h = Hasher::new("pixels");
            for p in run {
                for c in p {
                    h.f32(*c);
                }
            }
            h.id().0
        })
        .collect();
    let mut h = Hasher::new("effect result");
    h.text(name);
    h.all(values, |h, v| v.ident(h));
    h.affine(transform);
    h.u32(u32::from(pixels.width()));
    h.u32(u32::from(pixels.height()));
    h.all(&runs, |h, run| h.id_of(Id(*run)));
    h.id()
}

/// What may be kept: a sixteenth of the budget in force. Results held
/// count against the budget that lets frames be drawn side by side, so a
/// larger share keeps frames waiting; recurring results recur in frames
/// close together, which a few held results serve.
fn share() -> u64 {
    memory::effective() / 16
}

/// The largest result kept: half of what may be. Larger grids, such as
/// planes far past the canvas, are seldom drawn twice with the same pixels,
/// and hashing and copying one costs a good part of working it out.
fn largest() -> u64 {
    share() / 2
}

/// Lets the least recently used results go until `more` bytes fit.
fn make_room(store: &mut Store, more: u64) -> bool {
    while store.bytes + more > share() {
        let Some(oldest) = store
            .results
            .iter()
            .min_by_key(|(_, kept)| kept.used)
            .map(|(id, _)| *id)
        else {
            return false;
        };
        if let Some(kept) = store.results.remove(&oldest) {
            let bytes = held(&kept.pixels);
            store.bytes -= bytes;
            memory::keep(bytes, false);
            memory::note_eviction();
        }
    }
    true
}

/// The result kept under `key`, if one is.
fn found(key: Id) -> Option<Arc<Pixels>> {
    let mut guard = store();
    let store = guard.get_or_insert_with(Store::default);
    // Pressure may have lowered the budget since: what no longer fits goes.
    make_room(store, 0);
    store.clock += 1;
    let clock = store.clock;
    let kept = store.results.get_mut(&key)?;
    kept.used = clock;
    Some(Arc::clone(&kept.pixels))
}

/// Keeps `pixels` under `key` if they fit.
fn keep(key: Id, pixels: &Pixels) {
    let bytes = held(pixels);
    let mut guard = store();
    let store = guard.get_or_insert_with(Store::default);
    if store.results.contains_key(&key) || bytes > largest() || !make_room(store, bytes) {
        return;
    }
    store.clock += 1;
    let used = store.clock;
    store.bytes += bytes;
    memory::keep(bytes, true);
    store.results.insert(
        key,
        Kept {
            pixels: Arc::new(pixels.clone()),
            used,
        },
    );
}

/// Runs `work` on `pixels`, or gives the result kept under the key of
/// `name`, `values` and `transform` on them: the same bits either way.
///
/// # Panics
/// In verify mode, when a kept result differs from working it out again.
pub(crate) fn run(
    name: &'static str,
    values: &[ParamValue],
    transform: vello_cpu::kurbo::Affine,
    pixels: &mut Pixels,
    work: impl FnOnce(&mut Pixels),
) {
    // A result too large to keep is never found either: it is worked out
    // without hashing what it reads.
    if held(pixels) > largest() {
        if let Some(store) = store().as_mut() {
            make_room(store, 0);
        }
        work(pixels);
        return;
    }
    let key = key(name, values, transform, pixels);
    match found(key) {
        Some(kept) if crate::identity::verify_hit() => {
            work(pixels);
            assert!(
                same_bits(pixels.data(), kept.data()),
                "`{name}`'s result kept under {:032x} differs from working it out again (D93)",
                key.0
            );
            REUSED.fetch_add(1, Relaxed);
        }
        Some(kept) => {
            crate::par::each_with(pixels.data_mut(), kept.data(), |_, p, k| *p = *k);
            REUSED.fetch_add(1, Relaxed);
        }
        None => {
            work(pixels);
            keep(key, pixels);
        }
    }
}

/// What `pixels` hold, in bytes of the budget.
fn held(pixels: &Pixels) -> u64 {
    pixels.data().len() as u64 * memory::PIXEL_BYTES
}

/// Whether `a` and `b` hold the same bits, pixel for pixel.
fn same_bits(a: &[Pixel], b: &[Pixel]) -> bool {
    a.len() == b.len()
        && a.par_iter()
            .zip(b)
            .all(|(a, b)| a.map(f32::to_bits) == b.map(f32::to_bits))
}

#[cfg(test)]
#[path = "../tests/unit/results.rs"]
mod tests;
