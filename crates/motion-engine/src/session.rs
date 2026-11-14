//! A scene being worked on: read once, compiled on demand, edited and saved.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use motion_edit::{apply_edit, parse_edit};
use motion_format::units::Time;
use motion_format::{Diagnostic, Kind, Result};
use motion_format::{Document, Scene, parse_document, write_document};
use motion_query::{Report, at, check};
use motion_scene::{FrameState, evaluate};
use motion_scene::{SceneGraph, compile};
use motion_text::{FontSet, Typesetter};

use crate::stage::{stage, stage_ok};

/// One scene and the results of the steps run on it. Methods run their steps
/// one after another, never nested, so each step is timed on its own.
#[derive(Debug)]
pub struct Session {
    /// The file the scene was read from, and is saved to.
    path: Option<PathBuf>,
    /// The scene as read or last edited.
    doc: Document,
    /// The fonts the scene's text is set in: the built-in ones and those its
    /// `fonts` names, read on first use and dropped by an edit.
    fonts: OnceLock<Result<FontSet>>,
    /// The compiled scene, built on first use and dropped by an edit.
    graph: OnceLock<Result<SceneGraph>>,
}

impl Session {
    /// Reads and parses the scene file at `path` (steps `read` and `parse`).
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be read, or the first
    /// problem in the scene.
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
        self.doc.scene()
    }

    /// The fonts (step `fonts`): Inter, which is built in, and every file and
    /// folder the scene's `fonts` names, relative to the scene file's folder
    /// (the working folder for a scene without a file). Read once per edit.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic for a font that cannot be read, at its place
    /// in `fonts`, or one for a file that is not a font.
    pub fn fonts(&self) -> Result<&FontSet> {
        self.fonts
            .get_or_init(|| stage("fonts", || self.read_fonts()))
            .as_ref()
            .map_err(Clone::clone)
    }

    fn read_fonts(&self) -> Result<FontSet> {
        let base = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(Path::new(""));
        let mut fonts = FontSet::builtin();
        for (i, entry) in self.doc.scene().fonts.iter().enumerate() {
            let path = base.join(entry);
            let added = match path.is_dir() {
                true => fonts.add_folder(&path),
                false => fonts.add_file(&path),
            };
            added.map_err(|e| e.at(format!("{}[{i}]", motion_format::key::FONTS)))?;
        }
        Ok(fonts)
    }

    /// The compiled scene (step `compile`), built once per edit.
    ///
    /// # Errors
    /// A font that cannot be read, or text whose font or glyphs the fonts lack.
    pub fn graph(&self) -> Result<&SceneGraph> {
        self.graph
            .get_or_init(|| {
                let mut typesetter = Typesetter::new(self.fonts()?.clone());
                stage("compile", || compile(self.doc.scene(), &mut typesetter))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Every node's values at `frame`, possibly fractional (step `evaluate`).
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub fn frame(&self, frame: f64) -> Result<FrameState> {
        let graph = self.graph()?;
        Ok(stage_ok("evaluate", || evaluate(graph, frame)))
    }

    /// The check report over every frame (step `check`).
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub fn check(&self) -> Result<Report> {
        let graph = self.graph()?;
        Ok(stage_ok("check", || check(self.doc.scene(), graph)))
    }

    /// Where the layers in `ids` (all when empty) are at `time`, such as
    /// `1.5s` or `45f` (steps `time` and `at`).
    ///
    /// # Errors
    /// A diagnostic for a bad or negative time, or an id that is not a layer.
    pub fn at(&self, time: &str, ids: &[String]) -> Result<String> {
        let fps = self.doc.scene().fps;
        let frame = stage("time", || Ok(time.parse::<Time>()?.to_frames(fps)))?;
        let graph = self.graph()?;
        stage("at", || at(graph, frame, ids))
    }

    /// Applies the edit `line` (step `edit`) and returns the new check report.
    /// Nothing is written until [`save`](Session::save).
    ///
    /// # Errors
    /// A diagnostic for a bad line or an invalid result; the session is then
    /// left as it was.
    pub fn edit(&mut self, line: &str) -> Result<Report> {
        self.doc = stage("edit", || apply_edit(&self.doc, &parse_edit(line)?))?;
        self.fonts = OnceLock::new();
        self.graph = OnceLock::new();
        self.check()
    }

    /// Writes the scene, in canonical form, back to its file (step `save`),
    /// through a renamed temp file so a failed write never leaves half a scene.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be written, or a
    /// [`Kind::Missing`] one when the session has no file.
    pub fn save(&self) -> Result<()> {
        stage("save", || {
            let path = self.path.as_deref().ok_or_else(|| {
                Diagnostic::new(Kind::Missing, "the scene has no file to save to")
                    .expected("a session from `Session::open`")
            })?;
            let tmp = temp_path(path);
            fs::write(&tmp, write_document(self.doc.raw()))
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

    /// Parses scene JSON that has no file (step `parse`), so cannot be saved.
    fn from_str(text: &str) -> Result<Self> {
        let doc = stage("parse", || parse_document(text))?;
        Ok(Self {
            path: None,
            doc,
            fonts: OnceLock::new(),
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
