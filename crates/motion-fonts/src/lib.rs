//! Fonts for motion scenes, resolved before a render and never during one
//! ([D39]).
//!
//! - A [`Lock`] names the exact file of every face a scene uses: where it
//!   comes from, its version, its licence and its [`Sha256`].
//! - The [`Cache`] keeps font files by their hash. A render loads a locked
//!   face from it and checks the hash; it never fetches.
//! - With the `fetch` feature, `fetch` finds the files: it lists the
//!   releases of each family's sources, which are data, downloads the
//!   candidates into the cache and keeps, for each family, the release a
//!   [`Judge`] scores best against what a project stored. Only open
//!   licences are fetched.
//!
//! [D39]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render

mod cache;
mod hash;
mod judge;
mod lock;

#[cfg(feature = "fetch")]
pub mod fetch;

pub use cache::{CACHE_VARIABLE, Cache};
pub use hash::Sha256;
pub use judge::{FontFile, Judge, LINE_OFF_CAP, LinesScore, Score, TextScore};
pub use lock::{LOCK_VERSION, Licence, Lock, LockedFont, Origin};
