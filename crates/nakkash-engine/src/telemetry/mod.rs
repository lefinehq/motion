//! The stderr subscriber that binaries and tests install to show logs and
//! step timings. Library code only emits `tracing` events, so stdout carries
//! nothing but command output.

mod init;
pub mod timing;

pub use init::{ENV_VAR, Format, Guard, OLD_ENV_VAR, Options, init};
/// The target of every deprecation note ([D73]), shown whatever the level.
///
/// [D73]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d73-the-project-is-named-nakkash
pub use nakkash_format::deprecated::TARGET as DEPRECATED_TARGET;
pub use tracing_subscriber::filter::LevelFilter;
