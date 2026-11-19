//! The one entry point to the nakkash pipeline. A [`Session`] reads, compiles,
//! queries, edits and renders a scene, and [`import`] reads an `.aep`
//! project, running every step in a `tracing` span.
//!
//! ```
//! use nakkash_engine::Session;
//!
//! let scene = r#"{"nakkash":1,"dur":"1s","layers":[{"id":"card","rect":"100x100"}]}"#;
//! let mut session: Session = scene.parse().unwrap();
//! assert_eq!(session.check().unwrap().to_string(), "OK 2");
//! let report = session.edit("card at=\"center 2000\"").unwrap();
//! assert!(report.has_errors());
//! ```

mod clips;
mod diff;
mod export;
mod fidelity;
mod fonts;
mod footage;
mod gates;
mod geometry;
#[cfg(feature = "gpu")]
pub mod gpu;
mod import;
mod level;
mod mix;
mod pictures;
mod relocate;
mod render;
mod session;
mod sounds;
mod stage;
pub mod telemetry;

#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;

pub use diff::{Diff, DiffOptions, Error as DiffError, Floor, FrameDiff, Pooled};
pub use export::export_project;
pub use fidelity::{
    Change, Floors, Ledger, Manifest, Measure, NOISE, Rule, Sample, Scoreboard, Verdict, Window,
    compare, measure,
};
pub use fonts::{
    LOCK_EXTENSION, LayoutJudge, add_fonts_entry, font_names, lock_path, missing_fonts,
    unnamed_fonts,
};
pub use geometry::{
    LineCheck, LineGeometry, TEXT_TOLERANCE, TextCheck, TextGeometry, check_text, written_text_size,
};
pub use import::{import, import_scene, is_template, measure_layouts, substitute_fonts};
pub use nakkash_import::export::{Change as ExportChange, Plan as ExportPlan};
pub use session::{BAKED_FOLDER, Session, limit_threads};

pub use nakkash_format::Id;
pub use nakkash_format::address::{json_at, key_text};
pub use nakkash_format::roles::Role;
pub use nakkash_format::shorthand::Font;
pub use nakkash_format::slots::{SEEN, is_reference, resolved};
pub use nakkash_format::{
    Address, Content, Diagnostic, Document, Kind, Listed, Lock, Result, RoleTarget, Scene,
    write_document,
};
pub use nakkash_format::{SlotType, color, colors, oklch};
pub use nakkash_import::map::{Import, Mapping, Options as ImportOptions, StoredText};
pub use nakkash_import::{Project, Report as ImportReport};
pub use nakkash_query::{Report, Severity};
pub use nakkash_raster::{Accelerator, Frame, Image, Quality, RenderOptions, Simd, Tally};
pub use nakkash_scene::{ContentState, FrameState, SceneGraph};
