use super::*;

/// A length no other test makes, so the pool's grids of it are this test's.
const ODD: usize = POOLED_LEAST + 4_093;

#[test]
fn a_freed_grid_is_handed_out_again_cleared_or_clobbered() {
    set_clobber(true);
    let mut first = grid(ODD, Start::Written);
    first.iter_mut().for_each(|p| *p = [0.25; 4]);
    let reused = stats().reused;
    release(first);
    // Written: the caller writes it all, so it holds what clobber mode
    // put there, never the last user's pixels.
    let second = grid(ODD, Start::Written);
    assert!(stats().reused > reused);
    assert!(
        second
            .iter()
            .all(|p| p.iter().all(|c| c.to_bits() == CLOBBER.to_bits()))
    );
    release(second);
    // Clear: transparent, whatever it held.
    let third = grid(ODD, Start::Clear);
    assert!(third.iter().all(|p| *p == [0.0; 4]));
    release(third);
    set_clobber(false);
}

#[test]
fn small_grids_are_left_to_the_allocator() {
    let small = grid(POOLED_LEAST - 1, Start::Written);
    let pooled = LEDGER.pooled.load(Relaxed);
    release(small);
    assert!(LEDGER.pooled.load(Relaxed) <= pooled);
}
