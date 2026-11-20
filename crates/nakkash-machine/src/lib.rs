//! What the machine a render runs on holds and how it is doing ([D90]):
//! how much memory a render may plan with, how hard the machine is pressed
//! for it, and what the process used, so a render stays inside its budget,
//! reacts before the system swaps and says what it cost.
//!
//! The figures come from the operating system: on Linux from `/proc` and
//! the control group (cgroup) the process runs in, which is how a container
//! is limited; on macOS from the kernel's variables. Where the system does
//! not say, the answer is `None` or the calm one, never a guess.
//!
//! Besides `nakkash-videotoolbox`, this is the one crate allowed unsafe
//! code ([D48]): the few calls into the C library that have no safe form,
//! each in a block that says why it is sound. Its API is safe.
//!
//! [D48]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d48-unsafe-code-lives-only-in-platform-bindings
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::num::NonZero;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

// Each platform reads some of these; the tests read all of them.
#[allow(dead_code, reason = "each platform reads some parsers, the tests all")]
mod parse;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;

mod alloc;

pub use alloc::{BIG, GivesBack};
pub use parse::Psi;

/// One mebibyte.
pub const MIB: u64 = 1 << 20;

/// The most the default budget is: 4 GiB, whatever the machine holds.
pub const DEFAULT_BUDGET_MOST: u64 = 4 << 30;

/// The budget when the machine's memory is not known: 1 GiB.
pub const DEFAULT_BUDGET_UNKNOWN: u64 = 1 << 30;

/// How hard the machine is pressed for memory, calmest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Pressure {
    /// Memory is to be had.
    #[default]
    Normal,
    /// Memory is getting short: a render keeps less.
    Warning,
    /// The system is about to swap or is swapping: a render keeps nothing
    /// it can draw again.
    Critical,
}

/// The memory a render may plan with, in bytes: the machine's physical
/// memory, or the limit of the container the process runs in when that is
/// lower. `None` when the system does not say.
pub fn memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    return linux::memory();
    #[cfg(target_os = "macos")]
    return macos::memory();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return None;
}

/// The budget of a render not given one: a quarter of `memory`, at most
/// [`DEFAULT_BUDGET_MOST`], or [`DEFAULT_BUDGET_UNKNOWN`] when the memory is
/// not known.
///
/// ```
/// use nakkash_machine::{budget_for, MIB};
/// assert_eq!(budget_for(Some(8 << 30)), 2 << 30);
/// assert_eq!(budget_for(Some(64 << 30)), 4 << 30);
/// assert_eq!(budget_for(None), 1 << 30);
/// assert_eq!(budget_for(Some(512 * MIB)), 128 * MIB);
/// ```
pub fn budget_for(memory: Option<u64>) -> u64 {
    match memory {
        Some(bytes) => (bytes / 4).min(DEFAULT_BUDGET_MOST),
        None => DEFAULT_BUDGET_UNKNOWN,
    }
}

/// The budget of a render on this machine not given one ([`budget_for`]).
pub fn default_budget() -> u64 {
    budget_for(memory())
}

/// How many threads a render uses when not told: on macOS its performance
/// cores, so the efficiency cores stay free for the desktop; elsewhere the
/// processors the process may use, which on Linux are its job's or its
/// container's. A process whose environment caps its threads uses no more:
/// `NAKKASH_THREADS`, else `RAYON_NUM_THREADS` (what a job runner sets for
/// a child it starts without `--threads`). Never more than the process may
/// use, never 0.
pub fn default_threads() -> usize {
    let usable = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    #[cfg(target_os = "macos")]
    let usable = macos::performance_cores().map_or(usable, |cores| cores.min(usable));
    let cap = |name: &str| std::env::var(name).ok();
    capped(
        usable,
        cap(THREADS_VAR).or_else(|| cap(RAYON_VAR)).as_deref(),
    )
}

/// The most threads the process's work uses, as [`set_thread_limit`] set
/// it; 0 when it set none.
static THREAD_LIMIT: AtomicUsize = AtomicUsize::new(0);

/// Caps the threads every part of this process works on from now on, or,
/// with `None`, takes the cap away: drawing, checking, comparing and
/// encoding all ask [`threads`]. What `--threads` sets.
pub fn set_thread_limit(limit: Option<NonZero<usize>>) {
    THREAD_LIMIT.store(limit.map_or(0, NonZero::get), Ordering::Relaxed);
}

/// The cap [`set_thread_limit`] set, if any.
pub fn thread_limit() -> Option<NonZero<usize>> {
    NonZero::new(THREAD_LIMIT.load(Ordering::Relaxed))
}

/// How many threads a part of this process works on: the cap
/// [`set_thread_limit`] set, else [`default_threads`]. Every pool and every
/// fan-out of threads in the workspace is this size, so one cap holds them
/// all.
pub fn threads() -> usize {
    thread_limit().map_or_else(default_threads, NonZero::get)
}

/// The variable that caps a process's threads.
pub const THREADS_VAR: &str = "NAKKASH_THREADS";

/// The thread pool library's own cap, read when [`THREADS_VAR`] is not set.
const RAYON_VAR: &str = "RAYON_NUM_THREADS";

/// `usable` threads, at most the whole number `cap` reads as when it is
/// one above 0; never 0.
fn capped(usable: usize, cap: Option<&str>) -> usize {
    let cap = cap
        .and_then(|c| c.trim().parse::<usize>().ok())
        .filter(|c| *c > 0);
    cap.map_or(usable, |c| c.min(usable)).max(1)
}

/// How hard the machine is pressed for memory now. On macOS, the kernel's
/// own level. On Linux, the hardest of three readings: the container's use
/// against its limit, its pressure stall information (or the machine's),
/// and the memory available against the total.
pub fn pressure() -> Pressure {
    #[cfg(target_os = "linux")]
    return linux::pressure();
    #[cfg(target_os = "macos")]
    return macos::pressure();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Pressure::Normal;
}

/// What this process has used so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Time on processors in user code.
    pub user: Duration,
    /// Time on processors in the kernel for it, page zeroing included.
    pub system: Duration,
    /// The most memory it held resident at once, in bytes.
    pub peak_resident: u64,
    /// Page faults that had to read a page in: from swap, or a file.
    pub major_faults: u64,
    /// Bytes it caused to be written to storage, when the system counts
    /// them.
    pub written: Option<u64>,
}

/// What this process has used so far; zero where the system does not say.
pub fn usage() -> Usage {
    #[cfg(unix)]
    {
        let mut usage = unix::rusage();
        #[cfg(target_os = "linux")]
        {
            usage.written = linux::written();
        }
        #[cfg(target_os = "macos")]
        {
            usage.written = macos::written();
        }
        usage
    }
    #[cfg(not(unix))]
    Usage::default()
}

/// The memory this process holds resident now, in bytes, when the system
/// says: what a budget is finally held to.
pub fn resident() -> Option<u64> {
    #[cfg(target_os = "linux")]
    return linux::resident();
    #[cfg(target_os = "macos")]
    return macos::resident();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return None;
}

/// The machine's swap, as its system counts it: machine-wide, since no
/// system says which process a swapped page was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swap {
    /// Pages swapped in and out since the machine started (Linux).
    Pages {
        /// Pages read back in.
        ins: u64,
        /// Pages written out.
        outs: u64,
    },
    /// Bytes of swap in use (macOS).
    Used(u64),
}

impl Swap {
    /// What changed from `before` to `self`, as a line: pages in and out,
    /// or the change in swap used.
    pub fn since(self, before: Swap) -> String {
        match (before, self) {
            (Swap::Pages { ins: i0, outs: o0 }, Swap::Pages { ins, outs }) => format!(
                "{} pages in, {} out (machine-wide)",
                ins.saturating_sub(i0),
                outs.saturating_sub(o0)
            ),
            (Swap::Used(b0), Swap::Used(b)) => {
                let change = i128::from(b) - i128::from(b0);
                format!(
                    "swap used {:+.1} MiB (machine-wide, {:.1} MiB now)",
                    change as f64 / MIB as f64,
                    b as f64 / MIB as f64
                )
            }
            _ => "unknown".to_owned(),
        }
    }
}

/// The machine's swap now, when its system says.
pub fn swap() -> Option<Swap> {
    #[cfg(target_os = "linux")]
    return linux::swap();
    #[cfg(target_os = "macos")]
    return macos::swap();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return None;
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
