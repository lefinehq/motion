//! A scene being worked on: read once, compiled on demand, edited and saved.

use std::fs;
use std::num::NonZero;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::thread;

use motion_edit::{apply_edit, parse_edit};
use motion_fonts::{Cache, Lock};
use motion_format::units::Time;
use motion_format::{Diagnostic, Kind, Result};
use motion_format::{Document, Scene, Value, parse_document, write_document};
use motion_query::{Report, at, check};
use motion_raster::{Frame, Image, RenderOptions};
use motion_scene::{FrameState, evaluate};
use motion_scene::{SceneGraph, compile};
use motion_text::{FontSet, Typesetter};

use crate::pictures::Pictures;
use crate::render::display_list;
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
    /// Where locked fonts are loaded from; this user's cache when unset.
    font_cache: Option<Cache>,
    /// The pictures image layers draw, each decoded once, on first use, and
    /// dropped by an edit.
    pictures: Pictures,
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
    /// An entry ending in `.lock` is a [`Lock`]: each face it pins is loaded
    /// from the font cache and checked against its hash ([D39]). A render
    /// never fetches a font.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic for a font that cannot be read, at its place
    /// in `fonts`, or one for a file that is not a font; for a lock, one for
    /// a face the cache lacks, naming the command that fetches it.
    ///
    /// [D39]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render
    pub fn fonts(&self) -> Result<&FontSet> {
        self.fonts
            .get_or_init(|| stage("fonts", || self.read_fonts()))
            .as_ref()
            .map_err(Clone::clone)
    }

    fn read_fonts(&self) -> Result<FontSet> {
        self.read_font_entries(true)
    }

    /// The fonts without those the scene's locks pin: the built-in ones and
    /// the files and folders its `fonts` names. What a fetch starts from, so
    /// it works on a scene whose lock is not fetched yet.
    ///
    /// # Errors
    /// As [`Session::fonts`], for the files and folders.
    pub fn unlocked_fonts(&self) -> Result<FontSet> {
        self.read_font_entries(false)
    }

    /// The locks the scene's `fonts` names, relative to the working folder.
    pub fn locks(&self) -> Vec<PathBuf> {
        let base = self.base();
        self.doc
            .scene()
            .fonts
            .iter()
            .map(|entry| base.join(entry))
            .filter(|path| is_lock(path))
            .collect()
    }

    /// The folder the scene's `fonts` and pictures are relative to.
    fn base(&self) -> &Path {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(Path::new(""))
    }

    fn read_font_entries(&self, locks: bool) -> Result<FontSet> {
        let base = self.base();
        let mut fonts = FontSet::builtin();
        for (i, entry) in self.doc.scene().fonts.iter().enumerate() {
            let path = base.join(entry);
            let added = match (is_lock(&path), path.is_dir()) {
                (true, _) if !locks => continue,
                (true, _) => self.add_locked(&mut fonts, &path),
                (false, true) => fonts.add_folder(&path),
                (false, false) => fonts.add_file(&path),
            };
            added.map_err(|e| e.at(format!("{}[{i}]", motion_format::key::FONTS)))?;
        }
        Ok(fonts)
    }

    /// Adds every face the lock at `path` pins, from the font cache.
    fn add_locked(&self, fonts: &mut FontSet, path: &Path) -> Result<usize> {
        let lock = Lock::read(path)?;
        let cache = match &self.font_cache {
            Some(cache) => cache.clone(),
            None => Cache::user()?,
        };
        let mut added = 0;
        let mut loaded = std::collections::BTreeSet::new();
        for font in &lock.fonts {
            if loaded.insert(font.sha256) {
                let bytes = cache.load(font)?;
                let source = format!("{} ({} {})", font.postscript, font.family, font.version);
                added += fonts.add_bytes(bytes, &source)?;
            }
        }
        Ok(added)
    }

    /// Loads locked fonts from `cache` instead of this user's cache.
    pub fn set_font_cache(&mut self, cache: Cache) {
        self.font_cache = Some(cache);
        self.fonts = OnceLock::new();
        self.graph = OnceLock::new();
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

    /// The display list of `frame`: what the rasteriser draws (steps
    /// `evaluate` and `display`, and `picture` for each picture file an
    /// image layer shows for the first time).
    ///
    /// Image layers' files are relative to the scene file's folder (the
    /// working folder for a scene without a file); each is read and decoded
    /// once per edit.
    ///
    /// # Errors
    /// As [`Session::graph`], an effect the rasteriser cannot draw, or a
    /// picture file that cannot be read or is not a PNG or JPEG file, at
    /// its layer's `image`.
    pub fn display_list(&self, frame: f64) -> Result<Frame> {
        let state = self.frame(frame)?;
        let graph = self.graph()?;
        let background = Value::from(self.doc.scene().background);
        let c = background.components();
        let base = self.base();
        let picture = |path: &str| self.pictures.get(&base.join(path));
        stage("display", || {
            display_list(graph, [c[0], c[1], c[2], c[3]], &state, &picture)
        })
    }

    /// Draws `frame`, possibly fractional, with the default options: precise
    /// colours and the processor's fastest instructions (steps `evaluate`,
    /// `display` and `render`).
    ///
    /// # Errors
    /// As [`Session::render_with`].
    pub fn render(&self, frame: f64) -> Result<Image> {
        self.render_with(frame, RenderOptions::default())
    }

    /// Draws `frame` with `options`; [`RenderOptions::GOLDEN`] draws the same
    /// bits on every processor.
    ///
    /// # Errors
    /// As [`Session::display_list`], or a canvas the rasteriser cannot hold
    /// (zero or above 65535 pixels a side).
    pub fn render_with(&self, frame: f64, options: RenderOptions) -> Result<Image> {
        let list = self.display_list(frame)?;
        let graph = self.graph()?;
        let (width, height) = (pixels(graph.width), pixels(graph.height));
        stage("render", || {
            motion_raster::render_with(&list, width, height, options)
        })
    }

    /// The whole frames the scene lasts: from 0 to its length, rounded up.
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub fn frames(&self) -> Result<Range<u32>> {
        Ok(0..self.graph()?.duration_frames.ceil() as u32)
    }

    /// Draws every frame of `frames`, one per processor thread at a time,
    /// and hands each image to `each` on the thread that drew it, in no set
    /// order. Step timings add up the threads' time.
    ///
    /// # Errors
    /// An out-of-range diagnostic for frames past [`Session::frames`], or
    /// the first failure by frame number, from drawing or from `each`, with
    /// the frame named in its message. Frames not started when one fails are
    /// left undrawn.
    pub fn render_frames(
        &self,
        frames: Range<u32>,
        options: RenderOptions,
        each: impl Fn(u32, Image) -> Result<()> + Sync,
    ) -> Result<()> {
        let scene = self.frames()?;
        if frames.start > frames.end || frames.end > scene.end {
            return Err(Diagnostic::new(
                Kind::OutOfRange,
                format!("frames {}..{}", frames.start, frames.end),
            )
            .expected(format!(
                "frames within the scene's 0..{} (an end past the last frame)",
                scene.end
            )));
        }
        let next = AtomicU32::new(frames.start);
        let stop = AtomicBool::new(false);
        let failure: Mutex<Option<(u32, Diagnostic)>> = Mutex::new(None);
        let workers = thread::available_parallelism()
            .map_or(1, NonZero::get)
            .min(frames.len().max(1));
        thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    while !stop.load(Ordering::Relaxed) {
                        let frame = next.fetch_add(1, Ordering::Relaxed);
                        if frame >= frames.end {
                            break;
                        }
                        let drawn = self
                            .render_with(f64::from(frame), options)
                            .and_then(|image| each(frame, image));
                        if let Err(mut d) = drawn {
                            stop.store(true, Ordering::Relaxed);
                            d.message = format!("frame {frame}: {}", d.message);
                            let mut first = failure.lock().unwrap_or_else(PoisonError::into_inner);
                            if first.as_ref().is_none_or(|(at, _)| frame < *at) {
                                *first = Some((frame, d));
                            }
                        }
                    }
                });
            }
        });
        match failure.into_inner().unwrap_or_else(PoisonError::into_inner) {
            Some((_, d)) => Err(d),
            None => Ok(()),
        }
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
        self.pictures = Pictures::default();
        self.check()
    }

    /// A copy of this session with the layers `ids` hidden, as an edit
    /// hides them; this one is left as it is.
    ///
    /// # Errors
    /// As [`Session::edit`], for an id the scene does not have.
    pub(crate) fn hiding(&self, ids: &[String]) -> Result<Session> {
        let mut hidden = Session {
            path: self.path.clone(),
            doc: self.doc.clone(),
            fonts: OnceLock::new(),
            graph: OnceLock::new(),
            font_cache: self.font_cache.clone(),
            pictures: Pictures::default(),
        };
        for id in ids {
            hidden.edit(&format!("{id} hidden=true"))?;
        }
        Ok(hidden)
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
            font_cache: None,
            pictures: Pictures::default(),
        })
    }
}

/// Whether a `fonts` entry is a lock.
fn is_lock(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e == crate::fonts::LOCK_EXTENSION)
}

/// A canvas side in whole pixels; validation keeps sizes whole and positive.
fn pixels(side: f64) -> u32 {
    side.round() as u32
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
