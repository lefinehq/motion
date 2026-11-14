//! A scene being worked on: read once, compiled on demand, edited and saved.
//!
//! [`Session`] runs the pipeline end to end: read and parse (step 1),
//! compile (step 2, cached until the next edit), evaluate (step 3), and the
//! `check`, `at` and edit side branches. Each method runs its steps through
//! the crate's `stage` wrapper one after another, never one inside another,
//! so each step's timing is its own.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use motion_diagnostic::{Diagnostic, Kind, Result};
use motion_edit::{apply_edit, parse_edit};
use motion_eval::{FrameState, evaluate};
use motion_format::units::Time;
use motion_format::{Document, Scene, parse_document, write_document};
use motion_query::{Report, at, check};
use motion_scene::{ApproxTextMeasure, SceneGraph, compile};

use crate::stage::{stage, stage_ok};

/// One scene document, with the results of the steps run on it so far.
///
/// The engine's entry point: [`open`](Session::open) a file, or
/// [`parse`](str::parse) scene JSON that did not come from one.
#[derive(Debug)]
pub struct Session {
    /// The file the scene was read from, and is saved to.
    path: Option<PathBuf>,
    /// The scene as read or last edited.
    doc: Document,
    /// The compiled scene, built on first use and dropped by an edit.
    graph: OnceLock<SceneGraph>,
}

impl Session {
    /// Reads and parses the scene file at `path` (steps `read` and `parse`).
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be read, or the
    /// diagnostic of the first problem in the scene.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let text = stage("read", || {
            fs::read_to_string(&path).map_err(|e| Diagnostic::io("read", &path, &e))
        })?;
        let mut session: Self = text.parse()?;
        session.path = Some(path);
        Ok(session)
    }

    /// The file this session reads from and saves to, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The validated scene.
    pub fn scene(&self) -> &Scene {
        &self.doc.scene
    }

    /// The compiled scene (step `compile`), built on the first call after
    /// opening or editing.
    pub fn graph(&self) -> &SceneGraph {
        self.graph
            .get_or_init(|| stage_ok("compile", || compile(&self.doc.scene, &ApproxTextMeasure)))
    }

    /// Every node's values at `frame`, in frames from the start, possibly
    /// fractional (step `evaluate`).
    pub fn frame(&self, frame: f64) -> FrameState {
        let graph = self.graph();
        stage_ok("evaluate", || evaluate(graph, frame))
    }

    /// The check report over every frame (step `check`).
    pub fn check(&self) -> Report {
        let graph = self.graph();
        stage_ok("check", || check(&self.doc.scene, graph))
    }

    /// Where the layers in `ids` (every layer when empty) are at `time`,
    /// written as seconds or frames such as `1.5s` or `45f` (steps `time`
    /// and `at`).
    ///
    /// # Errors
    /// A diagnostic for a time that cannot be parsed or is negative, or for
    /// an id that is not a layer.
    pub fn at(&self, time: &str, ids: &[String]) -> Result<String> {
        // Seconds need the scene's frame rate to become frames.
        let fps = self.doc.scene.fps;
        let frame = stage("time", || Ok(time.parse::<Time>()?.to_frames(fps)))?;
        let graph = self.graph();
        stage("at", || at(graph, frame, ids))
    }

    /// Applies the edit `line` (step `edit`), keeps the result and returns
    /// its check report. The file is not written until [`save`](Session::save).
    ///
    /// # Errors
    /// A diagnostic for a line that cannot be parsed or an edit whose result
    /// is not a valid scene; the session is then left as it was.
    pub fn edit(&mut self, line: &str) -> Result<Report> {
        self.doc = stage("edit", || apply_edit(&self.doc.raw, &parse_edit(line)?))?;
        // The old graph describes the old scene.
        self.graph = OnceLock::new();
        Ok(self.check())
    }

    /// Writes the scene, in canonical form, back to the file it was read
    /// from (step `save`).
    ///
    /// The text goes to a sibling temp file that is then renamed over the
    /// scene. A rename is atomic, so a failed write never leaves a
    /// half-written scene.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be written, or a
    /// [`Kind::Missing`] one when the session was not opened from a file.
    pub fn save(&self) -> Result<()> {
        stage("save", || {
            let path = self.path.as_deref().ok_or_else(|| {
                Diagnostic::new(Kind::Missing, "the scene has no file to save to")
                    .expected("a session from `Session::open`")
            })?;
            let tmp = temp_path(path);
            fs::write(&tmp, write_document(&self.doc.raw))
                .map_err(|e| Diagnostic::io("write", path, &e))?;
            fs::rename(&tmp, path).map_err(|e| {
                // Best effort: the rename error is the one worth reporting.
                let _ = fs::remove_file(&tmp);
                Diagnostic::io("write", path, &e)
            })
        })
    }
}

impl FromStr for Session {
    type Err = Diagnostic;

    /// Parses scene JSON that did not come from a file (step `parse`). Such a
    /// session cannot be [saved](Session::save).
    ///
    /// # Errors
    /// The diagnostic of the first problem in the scene.
    fn from_str(text: &str) -> Result<Self> {
        let doc = stage("parse", || parse_document(text))?;
        Ok(Self {
            path: None,
            doc,
            graph: OnceLock::new(),
        })
    }
}

/// The sibling file the new text is written to before it replaces `path`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
#[path = "../tests/unit/session.rs"]
mod tests;
