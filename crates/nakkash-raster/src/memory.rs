//! One byte budget for everything a render holds ([D90]): the float grids
//! being worked, the drawings kept for later uses, and a pool of freed
//! grids handed out again.
//!
//! Every [`Pixels`](crate::Pixels) counts its bytes here from when it is
//! made until it is dropped. A dropped grid of at least [`POOLED_LEAST`]
//! pixels goes to the pool while what is held stays within the budget, and
//! the next grid of its size is taken from there instead of from the
//! system: the system would hand back fresh pages, which it must zero, and
//! on Linux a 4K frame zeroed hundreds of GiB that way. A grid from the
//! pool holds what its last user left, so it is only handed out to code
//! that writes every pixel before reading any, or cleared first: nothing
//! here relies on zeroed memory. In clobber mode (`NAKKASH_CLOBBER`, and
//! [`set_clobber`] in tests) every pooled grid is filled with a pattern no
//! drawing makes before it is handed out, so a read of stale memory shows.
//!
//! Over the budget the render gets slower, never larger, and never other:
//! the pool lets its grids go and frames wait to start ([`over`]). Kept
//! drawings take at most half the budget, and are let go to be drawn again
//! beyond it. None of it changes a pixel.
//!
//! Memory pressure lowers the budget for a while ([`note_pressure`]): at a
//! warning to three quarters of what is held, at a critical level to
//! nothing kept at all. The budget comes back once the pressure has been
//! normal for [`CALM`]. A probe set by the caller ([`set_probe`]) is read
//! at most every [`PROBE_EVERY`], as grids are made and frames start; it
//! also says what the process holds resident, which is held to the budget
//! as well, since grids are not all a render holds.
//!
//! The counts are kept for the whole process; [`stats`] reads them.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::pixels::Pixel;

/// The bytes of one pixel of a float grid.
pub(crate) const PIXEL_BYTES: u64 = std::mem::size_of::<Pixel>() as u64;

/// The fewest pixels a grid has for the pool to keep it: a mebibyte. The
/// system's allocator reuses smaller blocks itself.
pub const POOLED_LEAST: usize = 1 << 16;

/// The budget until the caller sets one: 1 GiB. The engine sets the
/// machine's own before it draws.
pub const DEFAULT_BUDGET: u64 = 1 << 30;

/// How long the pressure must stay normal before the budget comes back.
pub const CALM: Duration = Duration::from_secs(3);

/// How often, at most, the pressure probe is read.
pub const PROBE_EVERY: Duration = Duration::from_millis(100);

/// The bit pattern clobber mode fills pooled grids with: a quiet NaN with
/// a payload no arithmetic produces, so any read of it shows in the bits.
const CLOBBER: f32 = f32::from_bits(0x7FC0_A5A5);

/// How hard the machine is pressed for memory, calmest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Pressure {
    /// Memory is to be had.
    #[default]
    Normal,
    /// Memory is getting short: keep less.
    Warning,
    /// The system is about to swap: keep nothing that can be drawn again.
    Critical,
}

/// What the budget saw over the process's renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryStats {
    /// The budget set, in bytes.
    pub budget: u64,
    /// The budget in force now, lowered while memory is pressed.
    pub effective: u64,
    /// The most held at once: grids being worked and grids pooled.
    pub peak: u64,
    /// The most the grids being worked alone held at once, beyond what the
    /// budget let the pool and kept drawings have.
    pub peak_working: u64,
    /// How often something was not kept, let go or waited for because the
    /// budget was reached.
    pub hits: u64,
    /// Drawings kept for later uses that were let go, to be drawn again.
    pub evictions: u64,
    /// Grids taken from the pool instead of the system.
    pub reused: u64,
    /// Grids made, from the pool or the system, of at least
    /// [`POOLED_LEAST`] pixels.
    pub made: u64,
    /// Pooled grids let go to keep within the budget.
    pub trimmed: u64,
    /// Times a frame waited to start because the budget was held.
    pub held_back: u64,
    /// Readings of the pressure probe above normal.
    pub pressed: u64,
}

/// The process's budget and its counts.
struct Ledger {
    budget: AtomicU64,
    effective: AtomicU64,
    /// Bytes of grids alive, kept drawings included.
    live: AtomicU64,
    /// Bytes of kept drawings, a part of `live`.
    kept: AtomicU64,
    /// Bytes of pooled grids, kept separately from `live`.
    pooled: AtomicU64,
    peak: AtomicU64,
    peak_working: AtomicU64,
    hits: AtomicU64,
    evictions: AtomicU64,
    reused: AtomicU64,
    made: AtomicU64,
    trimmed: AtomicU64,
    held_back: AtomicU64,
    pressed: AtomicU64,
    clobber: AtomicBool,
    /// When the probe was last read and when pressure was last seen, in
    /// milliseconds since the ledger was made.
    probed: AtomicU64,
    last_pressed: AtomicU64,
}

static LEDGER: Ledger = Ledger {
    budget: AtomicU64::new(DEFAULT_BUDGET),
    effective: AtomicU64::new(DEFAULT_BUDGET),
    live: AtomicU64::new(0),
    kept: AtomicU64::new(0),
    pooled: AtomicU64::new(0),
    peak: AtomicU64::new(0),
    peak_working: AtomicU64::new(0),
    hits: AtomicU64::new(0),
    evictions: AtomicU64::new(0),
    reused: AtomicU64::new(0),
    made: AtomicU64::new(0),
    trimmed: AtomicU64::new(0),
    held_back: AtomicU64::new(0),
    pressed: AtomicU64::new(0),
    clobber: AtomicBool::new(false),
    probed: AtomicU64::new(0),
    last_pressed: AtomicU64::new(0),
};

/// The freed grids, by their length in pixels.
static POOL: Mutex<Option<HashMap<usize, Vec<Vec<Pixel>>>>> = Mutex::new(None);

/// The probe the caller set.
static PROBE: OnceLock<fn() -> Reading> = OnceLock::new();

/// What the process held resident at the probe's last reading, in bytes;
/// 0 when unknown.
static RESIDENT: AtomicU64 = AtomicU64::new(0);

/// A reading of the machine: its memory pressure, and what the process
/// holds resident, when the system says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reading {
    /// How hard the machine is pressed for memory.
    pub pressure: Pressure,
    /// The bytes the process holds resident: everything it holds, not only
    /// the grids the budget counts.
    pub resident: Option<u64>,
}

/// When the ledger's clock started.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Milliseconds since the ledger's clock started.
fn now_ms() -> u64 {
    u64::try_from(epoch().elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn pool() -> MutexGuard<'static, Option<HashMap<usize, Vec<Vec<Pixel>>>>> {
    POOL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Sets the budget, in bytes, for every render after this; it takes effect
/// at once, and the pool lets go of what no longer fits.
pub fn set_budget(bytes: u64) {
    LEDGER.budget.store(bytes, Relaxed);
    LEDGER.effective.store(bytes, Relaxed);
    trim_to(bytes);
}

/// The budget set, in bytes.
pub fn budget() -> u64 {
    LEDGER.budget.load(Relaxed)
}

/// Turns clobber mode on or off: grids from the pool are filled with a
/// pattern no drawing makes before they are handed out, so code that reads
/// a pixel it did not write changes the frame's bits. For tests; also on
/// when `NAKKASH_CLOBBER` is set.
pub fn set_clobber(on: bool) {
    LEDGER.clobber.store(on, Relaxed);
}

fn clobbering() -> bool {
    static FROM_ENV: OnceLock<bool> = OnceLock::new();
    LEDGER.clobber.load(Relaxed)
        || *FROM_ENV.get_or_init(|| std::env::var_os("NAKKASH_CLOBBER").is_some())
}

/// Sets the probe read for memory pressure and what the process holds
/// resident, at most every [`PROBE_EVERY`]; the first one set stays. What
/// is resident is held to the budget too: above it, the render keeps and
/// pools nothing more and starts no frame beside another, so grids are not
/// the only memory the budget bounds.
pub fn set_probe(probe: fn() -> Reading) {
    let _ = PROBE.set(probe);
}

/// Reads the probe if it was not read in the last [`PROBE_EVERY`].
fn probe() {
    let Some(probe) = PROBE.get() else {
        return;
    };
    let now = now_ms();
    let last = LEDGER.probed.load(Relaxed);
    let every = u64::try_from(PROBE_EVERY.as_millis()).unwrap_or(100);
    if now.saturating_sub(last) < every
        || LEDGER
            .probed
            .compare_exchange(last, now, Relaxed, Relaxed)
            .is_err()
    {
        return;
    }
    let reading = probe();
    RESIDENT.store(reading.resident.unwrap_or(0), Relaxed);
    note_pressure(reading.pressure);
}

/// Whether what the process holds resident, at the last reading, is over
/// `bytes`.
fn resident_over(bytes: u64) -> bool {
    RESIDENT.load(Relaxed) > bytes
}

/// Takes in a reading of memory pressure: above normal, the budget in
/// force falls (to three quarters of what is held at a warning, to
/// nothing at a critical level) and the pool lets go of what no longer
/// fits; normal for [`CALM`], it comes back.
pub fn note_pressure(pressure: Pressure) {
    let now = now_ms();
    let budget = LEDGER.budget.load(Relaxed);
    match pressure {
        Pressure::Normal => {
            let calm = u64::try_from(CALM.as_millis()).unwrap_or(3000);
            let since = now.saturating_sub(LEDGER.last_pressed.load(Relaxed));
            if since >= calm {
                LEDGER.effective.store(budget, Relaxed);
            }
        }
        Pressure::Warning | Pressure::Critical => {
            LEDGER.pressed.fetch_add(1, Relaxed);
            LEDGER.last_pressed.store(now.max(1), Relaxed);
            let lowered = match pressure {
                Pressure::Critical => 0,
                _ => (held() / 4 * 3).min(budget),
            };
            LEDGER.effective.fetch_min(lowered, Relaxed);
            trim_to(LEDGER.effective.load(Relaxed));
        }
    }
}

/// What is held: grids alive and grids pooled.
fn held() -> u64 {
    LEDGER.live.load(Relaxed) + LEDGER.pooled.load(Relaxed)
}

fn note_peak() {
    let live = LEDGER.live.load(Relaxed);
    let total = live + LEDGER.pooled.load(Relaxed);
    LEDGER.peak.fetch_max(total, Relaxed);
    let working = live.saturating_sub(LEDGER.kept.load(Relaxed));
    LEDGER.peak_working.fetch_max(working, Relaxed);
}

/// Lets pooled grids go until what is held fits in `bytes`, or the pool is
/// empty.
fn trim_to(bytes: u64) {
    let mut freed = Vec::new();
    {
        let mut guard = pool();
        let Some(pool) = guard.as_mut() else {
            return;
        };
        while held() > bytes {
            // The largest first: fewest grids let go for the room made.
            let Some(&len) = pool.keys().max() else {
                break;
            };
            let grids = pool.get_mut(&len).expect("a key of the pool");
            let grid = grids.pop().expect("only lengths with grids are kept");
            if grids.is_empty() {
                pool.remove(&len);
            }
            LEDGER.pooled.fetch_sub(bytes_of(grid.len()), Relaxed);
            LEDGER.trimmed.fetch_add(1, Relaxed);
            freed.push(grid);
        }
    }
    // Given back to the system outside the lock.
    drop(freed);
}

fn bytes_of(len: usize) -> u64 {
    len as u64 * PIXEL_BYTES
}

/// Whether what is held is over the budget in force. The pool lets its
/// grids go first, so this is true only when grids being worked and kept
/// drawings hold the budget; a caller then lets kept drawings go, or waits.
pub fn over() -> bool {
    probe();
    let effective = LEDGER.effective.load(Relaxed);
    if held() <= effective && !resident_over(effective) {
        return false;
    }
    match resident_over(effective) {
        // Over by memory the budget does not count: the pool goes first.
        true => trim_to(0),
        false => trim_to(effective),
    }
    held() > effective || resident_over(effective)
}

/// What the process held resident at the last reading, in bytes; `None`
/// when unknown.
pub fn resident() -> Option<u64> {
    probe();
    Some(RESIDENT.load(Relaxed)).filter(|&bytes| bytes > 0)
}

/// Notes that a frame waited to start because the budget was held.
pub fn note_held_back() {
    LEDGER.held_back.fetch_add(1, Relaxed);
    LEDGER.hits.fetch_add(1, Relaxed);
}

/// What the budget saw so far.
pub fn stats() -> MemoryStats {
    MemoryStats {
        budget: LEDGER.budget.load(Relaxed),
        effective: LEDGER.effective.load(Relaxed),
        peak: LEDGER.peak.load(Relaxed),
        peak_working: LEDGER.peak_working.load(Relaxed),
        hits: LEDGER.hits.load(Relaxed),
        evictions: LEDGER.evictions.load(Relaxed),
        reused: LEDGER.reused.load(Relaxed),
        made: LEDGER.made.load(Relaxed),
        trimmed: LEDGER.trimmed.load(Relaxed),
        held_back: LEDGER.held_back.load(Relaxed),
        pressed: LEDGER.pressed.load(Relaxed),
    }
}

/// What a new grid of `len` pixels starts as.
#[derive(Clone, Copy)]
pub(crate) enum Start {
    /// Every pixel transparent.
    Clear,
    /// Whatever it holds: the caller writes every pixel before it reads
    /// one.
    Written,
}

/// A grid of `len` pixels, counted as alive: one from the pool when it has
/// one of that length, else a new one. With [`Start::Written`] a pooled
/// grid keeps what it held (clobbered in clobber mode), so the caller must
/// write every pixel.
pub(crate) fn grid(len: usize, start: Start) -> Vec<Pixel> {
    if len >= POOLED_LEAST {
        probe();
        LEDGER.made.fetch_add(1, Relaxed);
    }
    let pooled = match len >= POOLED_LEAST {
        true => pool().as_mut().and_then(|pool| {
            let grids = pool.get_mut(&len)?;
            let grid = grids.pop();
            if grids.is_empty() {
                pool.remove(&len);
            }
            grid
        }),
        false => None,
    };
    let grid = match pooled {
        Some(mut grid) => {
            LEDGER.pooled.fetch_sub(bytes_of(len), Relaxed);
            LEDGER.reused.fetch_add(1, Relaxed);
            match start {
                Start::Clear => crate::par::each(&mut grid, |_, p| *p = [0.0; 4]),
                Start::Written if clobbering() => {
                    crate::par::each(&mut grid, |_, p| *p = [CLOBBER; 4])
                }
                Start::Written => {}
            }
            grid
        }
        None => vec![[0.0; 4]; len],
    };
    made(len);
    grid
}

thread_local! {
    /// The bytes of grids this thread made and has not let go since its
    /// frame started, and their most: a frame is drawn by its thread, which
    /// makes its grids even when other threads work its rows.
    static FRAME: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) };
}

/// Starts counting the grids of a frame this thread draws.
pub fn frame_start() {
    FRAME.with(|frame| frame.set((0, 0)));
}

/// The most bytes of grids the frame this thread draws held at once,
/// since [`frame_start`]: what one frame costs, for deciding how many
/// frames fit the budget side by side.
pub fn frame_peak() -> u64 {
    FRAME.with(|frame| frame.get().1)
}

/// Counts a grid of `len` pixels as alive, however it was made.
pub(crate) fn made(len: usize) {
    LEDGER.live.fetch_add(bytes_of(len), Relaxed);
    FRAME.with(|frame| {
        let (live, peak) = frame.get();
        let live = live + bytes_of(len);
        frame.set((live, peak.max(live)));
    });
    note_peak();
}

/// Takes back a grid no longer alive: into the pool when it is large
/// enough and what is held stays within the budget, else to the system.
pub(crate) fn release(grid: Vec<Pixel>) {
    let len = grid.len();
    LEDGER.live.fetch_sub(bytes_of(len), Relaxed);
    FRAME.with(|frame| {
        let (live, peak) = frame.get();
        frame.set((live.saturating_sub(bytes_of(len)), peak));
    });
    if len < POOLED_LEAST || grid.capacity() != len {
        return;
    }
    let effective = LEDGER.effective.load(Relaxed);
    if held() + bytes_of(len) > effective || resident_over(effective) {
        LEDGER.hits.fetch_add(1, Relaxed);
        return;
    }
    LEDGER.pooled.fetch_add(bytes_of(len), Relaxed);
    pool()
        .get_or_insert_with(HashMap::new)
        .entry(len)
        .or_default()
        .push(grid);
    note_peak();
}

/// Whether `bytes` more of drawings kept for later uses fit their share of
/// the budget in force: half of it. Kept drawings are what a frame would
/// otherwise draw again, at the cost of more working grids than they hold,
/// so they are bounded by their own share rather than let go whenever the
/// working grids pass the budget; memory pressure lowers the budget, and
/// so their share.
pub(crate) fn room_to_keep(bytes: u64) -> bool {
    probe();
    LEDGER.kept.load(Relaxed) + bytes <= LEDGER.effective.load(Relaxed) / 2
}

/// Counts `bytes` of drawings as kept for later uses, or no longer.
pub(crate) fn keep(bytes: u64, kept: bool) {
    match kept {
        true => LEDGER.kept.fetch_add(bytes, Relaxed),
        false => LEDGER.kept.fetch_sub(bytes, Relaxed),
    };
}

/// Notes a drawing that was not kept, or was let go, for the budget.
pub(crate) fn note_eviction() {
    LEDGER.evictions.fetch_add(1, Relaxed);
    LEDGER.hits.fetch_add(1, Relaxed);
}

#[cfg(test)]
#[path = "../tests/unit/memory.rs"]
mod tests;
