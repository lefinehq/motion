//! Changes a scene with one short line, `<target> <key>=<value> ...`.
//! [`parse_edit`] reads the line and [`apply_edit`] applies it, then
//! re-validates the scene, so an edit never leaves a broken scene behind.
//!
//! ```
//! let edit = motion_edit::parse_edit("title text=\"Hello\" in=none").unwrap();
//! assert_eq!(edit.target, "title");
//! assert_eq!(edit.changes.len(), 2);
//! ```

mod apply;
mod key_path;
mod parse;
mod path;
mod problem;

pub use apply::apply_edit;
pub use key_path::KeyPath;
pub use parse::{Change, Edit, EditValue, parse_edit};
