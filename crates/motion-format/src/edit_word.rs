//! The bare words of the edit language that are not strings.
//!
//! Part of step 1 (Read) only as spelling: the edit language lives in
//! `motion-edit`, but a validation hint in `crate::validate` writes an edit
//! line, so both spell these words from here.

/// Removes a key, as in `in=none`.
pub const NONE: &str = "none";
/// The JSON boolean `true`.
pub const TRUE: &str = "true";
/// The JSON boolean `false`.
pub const FALSE: &str = "false";
