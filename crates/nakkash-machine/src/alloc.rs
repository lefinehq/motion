//! An allocator that gives a big block back to the system when it is freed
//! ([D90]), for a program to install as its global allocator.
//!
//! A render holds its memory budget by letting grids go, and the budget
//! counts on what is let go leaving the process. The system allocators do
//! not always do that: on macOS the default allocator keeps freed blocks
//! of a 1080p grid's size (31.6 MiB) resident and charged to the process,
//! however many (1.24 GiB of them, measured on macOS 26), and glibc serves
//! blocks below its moving mapping threshold, up to 32 MiB, from a heap it
//! gives back only from the top. [`GivesBack`] maps every block of at
//! least [`BIG`] bytes from the system on its own and unmaps it when it is
//! freed, so what the process holds resident is what it uses; smaller
//! blocks go to the system allocator, which reuses them itself.
//!
//! A block it maps is zeroed by the system, so a zeroed allocation costs
//! nothing more; a block's contents are never read before they are
//! written, by its contract, so nothing a program computes changes.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::alloc::{GlobalAlloc, Layout, System};

/// The fewest bytes a block has for [`GivesBack`] to map it on its own: a
/// mebibyte, the size of the smallest grid the memory budget pools.
pub const BIG: usize = 1 << 20;

/// The global allocator that gives big blocks back to the system when
/// they are freed; see the [module](self). Installed with
/// `#[global_allocator] static ALLOCATOR: GivesBack = GivesBack;`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GivesBack;

/// The smallest memory page of the systems it maps on: a mapping is
/// aligned to at least this.
#[cfg(unix)]
const PAGE_LEAST: usize = 4096;

/// Whether `layout` is a block [`GivesBack`] maps on its own: big, and
/// aligned to no more than a page, which a mapping always is.
#[cfg(unix)]
fn mapped(layout: Layout) -> bool {
    layout.size() >= BIG && layout.align() <= PAGE_LEAST
}

#[cfg(not(unix))]
fn mapped(_: Layout) -> bool {
    false
}

// SAFETY: every block is either the system allocator's, for layouts
// `mapped` refuses, or a fresh private anonymous mapping of at least its
// size, page-aligned and so aligned to the layout, unmapped only by
// `dealloc` and `realloc` with the layout it was made with; `mapped` reads
// only the layout, so a block is always freed the way it was made.
unsafe impl GlobalAlloc for GivesBack {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match mapped(layout) {
            true => map(layout.size()),
            // SAFETY: the caller's layout, as `GlobalAlloc::alloc` requires.
            false => unsafe { System.alloc(layout) },
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        match mapped(layout) {
            // A fresh anonymous mapping reads as zeros.
            true => map(layout.size()),
            // SAFETY: the caller's layout, as `GlobalAlloc::alloc_zeroed`
            // requires.
            false => unsafe { System.alloc_zeroed(layout) },
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match mapped(layout) {
            // SAFETY: `ptr` was mapped by `map` with this layout's size, as
            // the caller made it with this layout.
            true => unsafe { unmap(ptr, layout.size()) },
            // SAFETY: the system allocator made `ptr` with this layout.
            false => unsafe { System.dealloc(ptr, layout) },
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller passes a size that, rounded up to the
        // alignment, does not overflow, as `GlobalAlloc::realloc` requires.
        let new = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        match (mapped(layout), mapped(new)) {
            // SAFETY: the system allocator made `ptr` with `layout`, and
            // keeps it if the new layout is too big for it.
            (false, false) => unsafe { System.realloc(ptr, layout, new_size) },
            #[cfg(target_os = "linux")]
            // SAFETY: `ptr` was mapped by `map` with `layout`'s size.
            (true, true) => unsafe { remap(ptr, layout.size(), new_size) },
            _ => {
                // SAFETY: `new` is a valid layout of a size above 0, as
                // `new_size` is.
                let to = unsafe { self.alloc(new) };
                if !to.is_null() {
                    let kept = layout.size().min(new_size);
                    // SAFETY: both blocks hold at least `kept` bytes, and a
                    // new block does not overlap one still held.
                    unsafe { std::ptr::copy_nonoverlapping(ptr, to, kept) };
                    // SAFETY: `ptr` was made by this allocator with
                    // `layout`, and is not used again.
                    unsafe { self.dealloc(ptr, layout) };
                }
                to
            }
        }
    }
}

/// A fresh private anonymous mapping of `size` bytes, read and written;
/// null if the system has none.
#[cfg(unix)]
fn map(size: usize) -> *mut u8 {
    // SAFETY: an anonymous mapping at an address the system picks touches
    // no memory the program holds.
    let at = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    match at == libc::MAP_FAILED {
        true => std::ptr::null_mut(),
        false => at.cast(),
    }
}

#[cfg(not(unix))]
fn map(_: usize) -> *mut u8 {
    std::ptr::null_mut()
}

/// Gives the mapping of `size` bytes at `ptr` back to the system.
///
/// # Safety
/// `ptr` and `size` are a mapping [`map`] made, not used again.
#[cfg(unix)]
unsafe fn unmap(ptr: *mut u8, size: usize) {
    // SAFETY: the caller's mapping, which nothing reads again. Unmapping a
    // mapping the process made cannot fail but for arguments it did not
    // make, so the result says nothing to act on.
    let _ = unsafe { libc::munmap(ptr.cast(), size) };
}

#[cfg(not(unix))]
unsafe fn unmap(_: *mut u8, _: usize) {}

/// The mapping of `old` bytes at `ptr` grown or shrunk to `new`, moved if
/// it must be; null, with the old mapping kept, if the system cannot.
///
/// # Safety
/// `ptr` and `old` are a mapping [`map`] made; on success it is not used
/// again.
#[cfg(target_os = "linux")]
unsafe fn remap(ptr: *mut u8, old: usize, new: usize) -> *mut u8 {
    // SAFETY: the caller's mapping; the system moves its pages, so no
    // byte is lost or read twice.
    let at = unsafe { libc::mremap(ptr.cast(), old, new, libc::MREMAP_MAYMOVE) };
    match at == libc::MAP_FAILED {
        true => std::ptr::null_mut(),
        false => at.cast(),
    }
}
