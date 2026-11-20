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
//! as well, since grids are not all a render holds. What it holds beside
//! the grids (its code, the scene, fonts, the rasteriser's own buffers,
//! frames kept to be copied) is taken off the room the grids have, at the
//! most it was seen to be, with the most it rose between two readings
//! kept free besides: the pool and kept drawings fill what is left, not
//! the whole budget.
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
    /// What is held now: grids being worked and grids pooled, in bytes.
    pub held: u64,
    /// The most the process was seen to hold resident beside the grids,
    /// in bytes: what the room for grids leaves out of the budget.
    pub outside: u64,
    /// The most what is resident beside the grids rose by between two
    /// readings, in bytes: kept free as well, since it may rise as far
    /// again before a reading sees it.
    pub rise: u64,
    /// The most frames drawn at once, as the caller notes them
    /// ([`note_frames_at_once`]).
    pub frames_at_once: u64,
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
    /// The most resident beside the grids held, at a reading.
    outside: AtomicU64,
    /// The most `outside` rose by from one reading that raised it to the
    /// next.
    rise: AtomicU64,
    frames_at_once: AtomicU64,
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
    outside: AtomicU64::new(0),
    rise: AtomicU64::new(0),
    frames_at_once: AtomicU64::new(0),
    clobber: AtomicBool::new(false),
    probed: AtomicU64::new(0),
    last_pressed: AtomicU64::new(0),
};

/// The freed grids, by their length in pixels.
static POOL: Mutex<Option<HashMap<usize, Vec<Vec<Pixel>>>>> = Mutex::new(None);

/// The probe the caller set.
static PROBE: OnceLock<fn() -> Reading> = OnceLock::new();

/// The reading of what is resident alone the caller set, quick enough to
/// read at every decision.
static RESIDENT_PROBE: OnceLock<fn() -> Option<u64>> = OnceLock::new();

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
    trim_to(room());
}

/// Starts the peaks [`stats`] reports over from what is held now: for a
/// measurement of one stretch of work. What is resident beside the grids
/// is learnt again from the next reading, and the most frames at once
/// counted again.
pub fn reset_peaks() {
    LEDGER.outside.store(0, Relaxed);
    LEDGER.rise.store(0, Relaxed);
    LEDGER.frames_at_once.store(0, Relaxed);
    let live = LEDGER.live.load(Relaxed);
    LEDGER
        .peak
        .store(live + LEDGER.pooled.load(Relaxed), Relaxed);
    LEDGER
        .peak_working
        .store(live.saturating_sub(LEDGER.kept.load(Relaxed)), Relaxed);
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

/// Sets a reading of what the process holds resident alone, read at every
/// decision of the budget (a grid made or let go, a frame about to start)
/// rather than every [`PROBE_EVERY`]: what the process holds beside its
/// grids changes between two readings of the whole probe (a frame's 8-bit
/// image, frames kept to be copied), and the room grids have follows it at
/// once. The first one set stays.
pub fn set_resident_probe(probe: fn() -> Option<u64>) {
    let _ = RESIDENT_PROBE.set(probe);
}

/// Reads what is resident, and the whole probe if it was not read in the
/// last [`PROBE_EVERY`].
fn probe() {
    if let Some(resident) = RESIDENT_PROBE.get() {
        note_resident(resident().unwrap_or(0));
    }
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
    if RESIDENT_PROBE.get().is_none() {
        note_resident(reading.resident.unwrap_or(0));
    }
    note_pressure(reading.pressure);
}

/// Takes in what the process holds resident, `bytes`, 0 when unknown, as
/// the probe reads it: what it holds beyond the grids counted is the most
/// seen so far, and the pool lets go of what no longer fits beside it.
pub fn note_resident(bytes: u64) {
    RESIDENT.store(bytes, Relaxed);
    if bytes == 0 {
        return;
    }
    // Grids are counted as held before their pages are resident and until
    // after they are given back, so what is resident beyond them is never
    // read as more than it is.
    let outside = bytes.saturating_sub(held());
    let before = LEDGER.outside.fetch_max(outside, Relaxed);
    if before < outside {
        // How far it rose since the last reading that raised it: it can
        // rise as far again before a reading sees it. The first reading
        // is where it starts from, not a rise.
        if before > 0 {
            LEDGER.rise.fetch_max(outside - before, Relaxed);
        }
        trim_to(room());
    }
}

/// The room grids have: the budget in force, less the most the process
/// was seen to hold resident beside them and the most that rose between
/// two readings, which it may rise by again before the next.
fn room() -> u64 {
    LEDGER
        .effective
        .load(Relaxed)
        .saturating_sub(LEDGER.outside.load(Relaxed))
        .saturating_sub(LEDGER.rise.load(Relaxed))
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
            trim_to(room());
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
    let mut freeing = 0;
    {
        let mut guard = pool();
        let Some(pool) = guard.as_mut() else {
            return;
        };
        while held().saturating_sub(freeing) > bytes {
            // The largest first: fewest grids let go for the room made.
            let Some(&len) = pool.keys().max() else {
                break;
            };
            let grids = pool.get_mut(&len).expect("a key of the pool");
            let grid = grids.pop().expect("only lengths with grids are kept");
            if grids.is_empty() {
                pool.remove(&len);
            }
            freeing += bytes_of(grid.len());
            LEDGER.trimmed.fetch_add(1, Relaxed);
            freed.push(grid);
        }
    }
    // Given back to the system outside the lock, and counted as gone once
    // they are.
    drop(freed);
    LEDGER.pooled.fetch_sub(freeing, Relaxed);
}

fn bytes_of(len: usize) -> u64 {
    len as u64 * PIXEL_BYTES
}

/// Whether what is held is over the room grids have in the budget in
/// force, or what is resident over the budget itself. The pool lets its
/// grids go first, so this is true only when grids being worked and kept
/// drawings hold that room; a caller then lets kept drawings go, or waits.
pub fn over() -> bool {
    probe();
    let effective = LEDGER.effective.load(Relaxed);
    if held() <= room() && !resident_over(effective) {
        return false;
    }
    match resident_over(effective) {
        // Over by memory the budget does not count: the pool goes first.
        true => trim_to(0),
        false => trim_to(room()),
    }
    held() > room() || resident_over(effective)
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

/// Notes that `frames` frames are being drawn at once.
pub fn note_frames_at_once(frames: u64) {
    LEDGER.frames_at_once.fetch_max(frames, Relaxed);
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
        held: held(),
        outside: LEDGER.outside.load(Relaxed),
        rise: LEDGER.rise.load(Relaxed),
        frames_at_once: LEDGER.frames_at_once.load(Relaxed),
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
            // Counted as alive before it stops counting as pooled, so it
            // is never held uncounted.
            count_made(len);
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
        None => {
            // A grid the pool has none of lets pooled grids of other sizes
            // go first, so the pool never sits beside new grids past the
            // room grids have (degradation's first step).
            if len >= POOLED_LEAST {
                let room = room();
                if held() + bytes_of(len) > room {
                    trim_to(room.saturating_sub(bytes_of(len)));
                }
            }
            count_made(len);
            vec![[0.0; 4]; len]
        }
    };
    note_peak();
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
    count_made(len);
    note_peak();
}

/// [`made`] without noting the peak, for a caller that notes it once the
/// grid is counted where it belongs.
fn count_made(len: usize) {
    LEDGER.live.fetch_add(bytes_of(len), Relaxed);
    FRAME.with(|frame| {
        let (live, peak) = frame.get();
        let live = live + bytes_of(len);
        frame.set((live, peak.max(live)));
    });
}

/// Takes back a grid no longer alive: into the pool when it is large
/// enough and what is held stays within the room grids have, else to the
/// system.
pub(crate) fn release(grid: Vec<Pixel>) {
    let len = grid.len();
    FRAME.with(|frame| {
        let (live, peak) = frame.get();
        frame.set((live.saturating_sub(bytes_of(len)), peak));
    });
    // Counted as alive until it is pooled or given back, so it is never
    // held uncounted.
    let gone = || LEDGER.live.fetch_sub(bytes_of(len), Relaxed);
    if len < POOLED_LEAST || grid.capacity() != len {
        drop(grid);
        gone();
        return;
    }
    probe();
    let effective = LEDGER.effective.load(Relaxed);
    // `held` counts this grid as alive already.
    if held() > room() || resident_over(effective) {
        LEDGER.hits.fetch_add(1, Relaxed);
        drop(grid);
        gone();
        return;
    }
    LEDGER.pooled.fetch_add(bytes_of(len), Relaxed);
    pool()
        .get_or_insert_with(HashMap::new)
        .entry(len)
        .or_default()
        .push(grid);
    gone();
    note_peak();
}

/// Whether `bytes` more of drawings kept for later uses fit their share of
/// the room grids have: half of it. Kept drawings are what a frame would
/// otherwise draw again, at the cost of more working grids than they hold,
/// so they are bounded by their own share rather than let go whenever the
/// working grids pass the budget; memory pressure lowers the budget, and
/// so their share.
pub(crate) fn room_to_keep(bytes: u64) -> bool {
    probe();
    LEDGER.kept.load(Relaxed) + bytes <= room() / 2
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
