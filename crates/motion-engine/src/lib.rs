//! The one entry point to the motion pipeline. A [`Session`] reads, compiles,
//! queries and edits a scene, running every step in a `tracing` span.
//!
//! ```
//! use motion_engine::Session;
//!
//! let scene = r#"{"motion":1,"dur":"1s","layers":[{"id":"card","rect":"100x100"}]}"#;
//! let mut session: Session = scene.parse().unwrap();
//! assert_eq!(session.check().to_string(), "OK 2");
//! let report = session.edit("card at=\"center 2000\"").unwrap();
//! assert!(report.has_errors());
//! ```

mod session;
mod stage;
pub mod telemetry;

pub use session::Session;

pub use motion_format::{Diagnostic, Document, Kind, Result, Scene};
pub use motion_query::Report;
pub use motion_scene::{FrameState, SceneGraph};
