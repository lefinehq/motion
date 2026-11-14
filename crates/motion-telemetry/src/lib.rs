//! `motion-telemetry`: logs and step timings for motion's binaries.
//!
//! Not a pipeline step. The library crates only emit `tracing` spans and
//! events; this crate installs the subscriber that shows them. It writes to
//! stderr only, so stdout carries nothing but command output. Binaries and
//! tests use it; library crates never do.
//!
//! - [`init`] installs the subscriber once, with a level, a format (text or
//!   one JSON object per line) and, on request, step timings.
//! - [`timing`] adds up how long each engine step took and prints the
//!   compact `timings parse=0.40ms check=2.30ms` line.
//!
//! The `tracing` macros are the API for emitting; this crate adds no macros
//! of its own.

mod init;
pub mod timing;

pub use init::{ENV_VAR, Format, Guard, Options, init};
pub use tracing_subscriber::filter::LevelFilter;
