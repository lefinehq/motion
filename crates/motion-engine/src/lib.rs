//! The one entry point to the motion pipeline. A [`Session`] reads, compiles,
//! queries, edits and renders a scene, and [`import`] reads an `.aep`
//! project, running every step in a `tracing` span.
//!
//! ```
//! use motion_engine::Session;
//!
//! let scene = r#"{"motion":1,"dur":"1s","layers":[{"id":"card","rect":"100x100"}]}"#;
//! let mut session: Session = scene.parse().unwrap();
//! assert_eq!(session.check().unwrap().to_string(), "OK 2");
//! let report = session.edit("card at=\"center 2000\"").unwrap();
//! assert!(report.has_errors());
//! ```

mod diff;
mod fidelity;
mod fonts;
mod geometry;
mod import;
mod render;
mod session;
mod stage;
pub mod telemetry;

#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;

pub use diff::{Diff, DiffOptions, Error as DiffError, FrameDiff, Pooled};
pub use fidelity::{Change, Ledger, Manifest, Measure, NOISE, Sample, Window, compare, measure};
pub use fonts::{
    LOCK_EXTENSION, LayoutJudge, add_fonts_entry, font_names, lock_path, missing_fonts,
};
pub use geometry::{LineCheck, LineGeometry, TEXT_TOLERANCE, TextCheck, TextGeometry, check_text};
pub use import::{import, import_scene};
pub use session::Session;

pub use motion_format::{Diagnostic, Document, Kind, Result, Scene, write_document};
pub use motion_import::map::{Import, Mapping, Options as ImportOptions, StoredText};
pub use motion_import::{Project, Report as ImportReport};
pub use motion_query::Report;
pub use motion_raster::{Frame, Image, Quality, RenderOptions, Simd};
pub use motion_scene::{FrameState, SceneGraph};
