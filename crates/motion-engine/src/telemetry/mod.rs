//! The stderr subscriber that binaries and tests install to show logs and
//! step timings. Library code only emits `tracing` events, so stdout carries
//! nothing but command output.

mod init;
pub mod timing;

pub use init::{ENV_VAR, Format, Guard, Options, init};
pub use tracing_subscriber::filter::LevelFilter;
