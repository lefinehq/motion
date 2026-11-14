//! Helpers shared by the unit tests.

use tracing::Subscriber;

/// Runs `body` with `subscriber` as the scoped default. Parallel tests share
/// tracing's global interest cache, so it is rebuilt for this subscriber.
pub(crate) fn with_subscriber<T>(
    subscriber: impl Subscriber + Send + Sync + 'static,
    body: impl FnOnce() -> T,
) -> T {
    tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        body()
    })
}
