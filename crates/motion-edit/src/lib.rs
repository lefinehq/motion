//! `motion-edit`: change a scene with one short line.
//!
//! This crate is a side branch of the pipeline, not a numbered rendering
//! step: it works on the same scene the earlier steps read (format, scene,
//! eval), next to `motion-query`. Where `motion-query` asks a
//! scene questions, this crate changes it: [`parse_edit`] turns a line into
//! an [`Edit`], and [`apply_edit`] applies it to the raw scene JSON and
//! re-validates the result with `motion-format`, so an edit can never leave
//! a broken scene behind.
//!
//! An edit line is `<target> <key>=<value> ...`:
//!
//! ```text
//! card fill=#FF0000 in=none
//! scene fps=60 bg="#0B0D12"
//! ```
//!
//! ```
//! let edit = motion_edit::parse_edit("title text=\"Hello\" in=none").unwrap();
//! assert_eq!(edit.target, "title");
//! assert_eq!(edit.changes.len(), 2);
//! ```

mod apply;
mod error;
mod parse;

pub use apply::apply_edit;
pub use error::EditError;
pub use parse::{Change, Edit, EditValue, parse_edit};
