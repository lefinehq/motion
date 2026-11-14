//! Helpers shared by the unit tests.

use std::sync::{Mutex, PoisonError};

use tracing::Subscriber;

/// Held while a test runs under a scoped subscriber.
///
/// Tracing keeps one global maximum level and interest cache for every
/// dispatcher in the process. `cargo test` runs tests as threads of one
/// process, and a subscriber that one test registers or drops recomputes
/// that state while another test's spans are being created, which can
/// briefly disable them (a timing test then sees only some of its steps).
/// Taking turns removes the race; tests that need no subscriber still run
/// in parallel.
static SCOPED: Mutex<()> = Mutex::new(());

/// Runs `body` with `subscriber` as the scoped default, one such test at a
/// time, with tracing's global interest cache rebuilt for this subscriber.
pub(crate) fn with_subscriber<T>(
    subscriber: impl Subscriber + Send + Sync + 'static,
    body: impl FnOnce() -> T,
) -> T {
    // A failed test poisons the lock; the next one may still run.
    let _turn = SCOPED.lock().unwrap_or_else(PoisonError::into_inner);
    tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        body()
    })
}
