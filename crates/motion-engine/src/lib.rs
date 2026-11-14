//! `motion-engine`: the one entry point to the motion pipeline.
//!
//! [`Session`] is the entry point, and the only way in: callers (the CLI
//! today; the MCP server and the Node binding later) open or parse a session
//! and call its methods. A session reads and parses a scene (step 1),
//! compiles it once on demand (step 2), evaluates frames (step 3) and runs
//! the `check`, `at` and edit side branches on it. Every step runs through
//! one private wrapper, so each one has a `tracing` span for logs and
//! timings, and every failure is logged once, in one place, before it
//! reaches the caller unchanged.
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
