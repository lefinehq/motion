//! [`GivesBack`] as the global allocator: a big block freed leaves the
//! process, and every block holds what was written to it, through growing
//! and shrinking across the size it maps from.
//!
//! What the process holds resident is the process's, so the two tests take
//! turns.

use std::sync::{Mutex, MutexGuard, PoisonError};

use nakkash_machine::{BIG, GivesBack, MIB};

#[global_allocator]
static ALLOCATOR: GivesBack = GivesBack;

/// One test at a time: what is resident is the process's.
fn turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The bytes of a 1080p grid of float pixels.
const GRID: usize = 1920 * 1080 * 16;

#[test]
fn a_freed_big_block_leaves_the_process() {
    let _turn = turn();
    let Some(before) = nakkash_machine::resident() else {
        // A system that does not say what is resident has nothing to hold.
        return;
    };
    // Twice: the system allocators keep blocks of this size after the
    // first time (macOS keeps every one, glibc moves its mapping threshold
    // past them).
    for round in 0..2 {
        let grids: Vec<Vec<u8>> = (0..8).map(|i| vec![i as u8 + 1; GRID]).collect();
        let held = nakkash_machine::resident().expect("resident, as before");
        assert!(
            held >= before + 7 * GRID as u64,
            "round {round}: {} MiB resident holding eight grids, {} MiB before",
            held / MIB,
            before / MIB
        );
        assert!(
            grids
                .iter()
                .enumerate()
                .all(|(i, g)| g[GRID - 1] == i as u8 + 1)
        );
        drop(grids);
        let after = nakkash_machine::resident().expect("resident, as before");
        assert!(
            after <= before + 16 * MIB,
            "round {round}: {} MiB resident once the grids were freed, {} MiB before",
            after / MIB,
            before / MIB
        );
    }
}

#[test]
fn blocks_keep_their_bytes_as_they_grow_and_shrink_across_the_mapped_size() {
    let _turn = turn();
    // Zeroed and mapped: reads as zeros.
    let zeros = vec![0u8; 3 * BIG];
    assert!(zeros.iter().all(|&b| b == 0));
    // Grown one byte at a time from the system allocator's sizes into
    // mapped ones, every byte kept.
    let mut grown = Vec::new();
    for i in 0..3 * BIG + 7 {
        grown.push((i % 251) as u8);
    }
    assert!(grown.iter().enumerate().all(|(i, &b)| b == (i % 251) as u8));
    // Mapped to mapped, larger: kept.
    grown.reserve_exact(5 * BIG);
    assert!(grown.capacity() >= 8 * BIG);
    assert!(grown.iter().enumerate().all(|(i, &b)| b == (i % 251) as u8));
    // Mapped to the system allocator's size: what is left is kept.
    grown.truncate(1000);
    grown.shrink_to_fit();
    assert!(grown.capacity() < BIG);
    assert!(grown.iter().enumerate().all(|(i, &b)| b == (i % 251) as u8));
    // A block aligned past a page is the system allocator's.
    #[repr(align(8192))]
    struct Paged([u8; 8192]);
    let paged: Vec<Paged> = (0..BIG / 8192 + 1).map(|_| Paged([9; 8192])).collect();
    assert!(paged.iter().all(|p| p.0[8191] == 9));
    assert_eq!(paged.as_ptr() as usize % 8192, 0);
}
