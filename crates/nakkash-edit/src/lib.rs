//! Changes a scene with one short line, `<target> <key>=<value> ...`.
//! [`parse_edit`] reads the line and [`apply_edit`] applies it, then
//! re-validates the scene, so an edit never leaves a broken scene behind.
//!
//! ```
//! let edit = nakkash_edit::parse_edit("title text=\"Hello\" in=none").unwrap();
//! assert_eq!(edit.target.to_string(), "title");
//! assert_eq!(edit.changes.len(), 2);
//! ```

mod apply;
pub mod instance;
mod key_path;
mod lock;
mod parse;
mod path;
mod problem;
mod role;
mod slot;

pub use apply::apply_edit;
pub use key_path::KeyPath;
pub use parse::{Change, Edit, EditValue, parse_edit};
pub use role::{edit_line, resolve_role};
