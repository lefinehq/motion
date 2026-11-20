//! The commands behind the `nakkash` binary; each one opens a
//! [`Session`](nakkash_engine::Session) and prints what it returns.

pub mod at;
pub mod batch;
pub mod bench;
pub mod brief;
pub mod check;
pub mod compact;
pub mod diff;
pub mod edit;
pub mod export;
pub mod fidelity;
pub mod fonts;
pub mod import;
pub mod inspect;
pub mod instance;
#[cfg(feature = "ml")]
pub mod ml;
pub mod render;
pub mod roles;
pub mod slots;
pub mod vary;

use std::process::ExitCode;

use nakkash_engine::Report;

/// Prints what `report` found that `base`, the report of an earlier version
/// of the scene, does not already have, after a line `KNOWN 3` counting what
/// it left out when that is any; exits 1 when what is printed has an error
/// line. An empty `base` prints the whole report.
pub fn print_new(mut report: Report, base: &Report) -> ExitCode {
    let known = report.since(base);
    if known > 0 {
        println!("KNOWN {known}");
    }
    println!("{report}");
    if report.has_errors() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
