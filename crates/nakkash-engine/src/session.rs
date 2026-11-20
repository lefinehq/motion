//! A scene being worked on: read once, compiled on demand, edited and saved.

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZero;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};

use rayon::{ThreadPool, ThreadPoolBuilder};

use nakkash_baked::Store;
use nakkash_fonts::{Cache, Lock};
use nakkash_format::units::Time;
use nakkash_format::{Address, Cutout, Diagnostic, Kind, Result};
use nakkash_format::{Document, Scene, parse_document, write_document};
use nakkash_query::{Checked, Report, at};
use nakkash_raster::{Accelerator, Frame, Image, RenderOptions, Tally};
use nakkash_scene::{FrameState, evaluate};
use nakkash_scene::{SceneGraph, compile};
use nakkash_text::{FontSet, Typesetter};

use crate::clips::Clips;
use crate::pictures::Pictures;
use crate::render::{Instances, Moments, Sources, display_list};
use crate::sounds::Sounds;
use crate::stage::{stage, stage_ok};

/// The folder of the cache that keeps baked assets ([D85]).
///
/// [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers
pub const BAKED_FOLDER: &str = "baked";

/// One scene and the results of the steps run on it. Methods run their steps
/// one after another, never nested, so each step is timed on its own.
#[derive(Debug)]
pub struct Session {
    /// The file the scene was read from, and is saved to.
    path: Option<PathBuf>,
    /// The instance the scene was merged from, if it was ([D91]): then
    /// `path` is the template's, which the session never writes.
    ///
    /// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
    instance: Option<PathBuf>,
    /// The scene as read or last edited.
    doc: Document,
    /// The fonts the scene's text is set in: the built-in ones and those its
    /// `fonts` names, read on first use and dropped by an edit.
    fonts: OnceLock<Result<FontSet>>,
    /// The compiled scene, built on first use and dropped by an edit;
    /// shared with the variants made of it.
    graph: OnceLock<Result<Arc<SceneGraph>>>,
    /// The scene's check, made on first use and dropped by an edit; shared
    /// with the variants made of it.
    checked: OnceLock<Arc<Checked>>,
    /// The last version of the scene that was checked before an edit, which
    /// the check after the edit starts from ([D79]).
    ///
    /// [D79]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d79-an-edit-checks-only-what-it-can-change
    before: Option<Arc<Before>>,
    /// Where locked fonts are loaded from; this user's cache when unset.
    font_cache: Option<Cache>,
    /// The pictures image layers draw, each decoded once, on first use, and
    /// dropped by an edit.
    pictures: Pictures,
    /// The clips video layers play, each opened once, on first use, and
    /// dropped by an edit.
    clips: Clips,
    /// The sounds audio layers play, each read once, on first use, and
    /// dropped by an edit.
    sounds: Sounds,
    /// What renders hand the float canvas's work to, if anything ([D74]):
    /// approximate frames, never the reference.
    ///
    /// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
    accelerator: Option<Arc<dyn Accelerator>>,
}

impl Session {
    /// Reads and parses the scene file at `path` (steps `read` and `parse`).
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be read, or the first
    /// problem in the scene.
    ///
    /// An instance ([D91]) opens as the scene it makes: its template with its
    /// values in it, read from the template's folder, or a diagnostic
    /// listing every error of the first stage that has any
    /// ([`crate::instance::open`] gives them as problems).
    ///
    /// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let text = stage("read", || {
            let text = fs::read_to_string(&path).map_err(|e| Diagnostic::io("read", &path, &e))?;
            crate::includes::flattened(&path, text)
        })?;
        if text.contains("\"nakkash.Instance.")
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
            && crate::instance::is_instance(&json)
        {
            let opened = crate::instance::open_json(&path, &json)?;
            for problem in opened
                .problems
                .iter()
                .filter(|p| p.level == crate::instance::Level::Warning)
            {
                tracing::warn!(target: "nakkash::instance", "{problem}");
            }
            return match opened.session {
                Some(session) if !crate::instance::has_errors(&opened.problems) => Ok(session),
                _ => Err(crate::instance::diagnostic(&opened)),
            };
        }
        let mut session: Self = text.parse()?;
        session.path = Some(path);
        Ok(session)
    }

    /// A session on `raw`, the scene an instance at `instance` makes of the
    /// template at `template` ([D91]).
    ///
    /// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
    ///
    /// # Errors
    /// The first problem in `raw` as a scene.
    pub(crate) fn on_instance(
        template: &Path,
        instance: &Path,
        raw: serde_json::Value,
    ) -> Result<Self> {
        let doc = stage("parse", || Document::from_value(raw))?;
        Ok(Self {
            path: Some(template.to_owned()),
            instance: Some(instance.to_owned()),
            doc,
            fonts: OnceLock::new(),
            graph: OnceLock::new(),
            checked: OnceLock::new(),
            before: None,
            font_cache: None,
            pictures: Pictures::default(),
            clips: Clips::default(),
            sounds: Sounds::default(),
            accelerator: None,
        })
    }

    /// The instance this session's scene was merged from, if any.
    pub fn instance_path(&self) -> Option<&Path> {
        self.instance.as_deref()
    }

    /// The file this session reads from and saves to, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The validated scene.
    pub fn scene(&self) -> &Scene {
        self.doc.scene()
    }

    /// Every slot of the scene, its own and then each composition's, with
    /// the values that follow it ([`Document::slots`]).
    pub fn slots(&self) -> Vec<nakkash_format::Listed> {
        self.doc.slots()
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
    /// [D39]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render
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

    /// The size in px of the picture `file`, relative to the scene's folder,
    /// decoded once and kept for the render; `None` when it is not a raster
    /// picture that reads.
    pub fn picture_size(&self, file: &str) -> Option<[f64; 2]> {
        match self.pictures.get(&self.base().join(file)).ok()? {
            crate::pictures::Still::Picture(picture) => {
                Some([f64::from(picture.width()), f64::from(picture.height())])
            }
            crate::pictures::Still::Drawing(_) => None,
        }
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
            added.map_err(|e| e.at(format!("{}[{i}]", nakkash_format::key::FONTS)))?;
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

    /// The store of baked assets video layers' cutouts read ([D85]): the
    /// `baked` folder of the cache fonts are loaded from.
    ///
    /// # Errors
    /// A [`Kind::Missing`](nakkash_format::Kind::Missing) diagnostic when no
    /// cache was given and the environment names no home.
    ///
    /// [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers
    pub fn baked_store(&self) -> Result<Store> {
        let cache = match &self.font_cache {
            Some(cache) => cache.clone(),
            None => Cache::user()?,
        };
        Ok(Store::new(cache.root().join(BAKED_FOLDER)))
    }

    /// Loads locked fonts from `cache` instead of this user's cache, and
    /// reads baked assets from its `baked` folder.
    pub fn set_font_cache(&mut self, cache: Cache) {
        self.font_cache = Some(cache);
        self.fonts = OnceLock::new();
        self.graph = OnceLock::new();
        self.checked = OnceLock::new();
        self.before = None;
    }

    /// The compiled scene (step `compile`), built once per edit. A scene
    /// whose tracks follow a sound compiles twice, measuring the levels
    /// between (step `listen`).
    ///
    /// # Errors
    /// A font that cannot be read, text whose font or glyphs the fonts
    /// lack, or a sound a listening track hears that cannot be read.
    pub fn graph(&self) -> Result<&SceneGraph> {
        self.graph
            .get_or_init(|| self.compile().map(Arc::new))
            .as_ref()
            .map(|graph| &**graph)
            .map_err(Clone::clone)
    }

    /// Compiles the scene, as [`Session::graph`] keeps it.
    fn compile(&self) -> Result<SceneGraph> {
        let mut typesetter = Typesetter::new(self.fonts()?.clone());
        let scene = self.doc.scene();
        let first = stage("compile", || compile(scene, &mut typesetter))?;
        // Tracks that follow a sound need the first compile's routes.
        let hearings = nakkash_scene::hearings(scene);
        if hearings.is_empty() {
            return Ok(first);
        }
        let base = self.base();
        let sound = |route: &nakkash_scene::Route| {
            self.sounds.get(&base.join(route.path())).map_err(|e| {
                e.at(nakkash_format::key::AUDIO)
                    .under(nakkash_format::Place::of_layer(route.id(&first)))
            })
        };
        let levels = stage("listen", || crate::level::levels(&first, &hearings, sound))?;
        stage("compile", || {
            nakkash_scene::compile_with(scene, &mut typesetter, &levels)
        })
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
    /// picture file that cannot be read or is not a PNG, JPEG or PDF file, at
    /// its layer's `image`.
    pub fn display_list(&self, frame: f64) -> Result<Frame> {
        self.list_of(&self.frame(frame)?)
    }

    /// The display list of `state`, a frame of the scene, perhaps with
    /// nodes hidden: as [`Session::display_list`].
    pub(crate) fn list_of(&self, state: &FrameState) -> Result<Frame> {
        self.list_in(self.graph()?, state)
    }

    /// The display list of `state` drawn by `graph`, the scene's graph or a
    /// copy of it with only what is drawn changed, such as its
    /// compositions' backgrounds: as [`Session::list_of`].
    pub(crate) fn list_in(&self, graph: &SceneGraph, state: &FrameState) -> Result<Frame> {
        let base = self.base();
        let pictures = |path: &str| self.pictures.get(&base.join(path));
        let store = || self.baked_store();
        let clips = |path: &str, cutout: Option<&Cutout>, seconds: f64| {
            self.clips.frame(&base.join(path), seconds, cutout, &store)
        };
        let sounds = |path: &str| self.sounds.get(&base.join(path));
        let moments = Moments::default();
        let instances = Instances::default();
        let sources = Sources {
            graph,
            pictures: &pictures,
            clips: &clips,
            sounds: &sounds,
            moments: &moments,
            instances: &instances,
        };
        stage("display", || display_list(sources, state))
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
        self.draw(&self.display_list(frame)?, options)
    }

    /// Draws `frame` at `scale` of the canvas's size, scaled as one picture,
    /// with `options`: what changed between two scenes shows at a fraction
    /// of a render's cost, though effects measured in pixels are not
    /// scaled with it.
    ///
    /// # Errors
    /// As [`Session::render_with`].
    pub fn render_small(&self, frame: f64, scale: f64, options: RenderOptions) -> Result<Image> {
        crate::gates::shrunk_by(self, self.display_list(frame)?, scale, options)
    }

    /// The canvas the scene is drawn on, in whole pixels.
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub(crate) fn canvas(&self) -> Result<(u32, u32)> {
        let graph = self.graph()?;
        Ok((pixels(graph.root.width), pixels(graph.root.height)))
    }

    /// Draws `list` on the scene's canvas, as one `render` step: every frame
    /// the engine draws goes through here.
    ///
    /// # Errors
    /// As [`Session::render_with`].
    pub(crate) fn draw(&self, list: &Frame, options: RenderOptions) -> Result<Image> {
        let (width, height) = self.canvas()?;
        stage("render", || {
            on_pool(|| {
                nakkash_raster::render_on(list, width, height, options, self.accelerator.as_deref())
            })
        })
    }

    /// Draws `list` on the scene's canvas on the CPU, whatever accelerator
    /// renders use: the reference, which fidelity is measured on ([D74]).
    ///
    /// # Errors
    /// As [`Session::render_with`].
    ///
    /// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
    pub(crate) fn draw_reference(&self, list: &Frame, options: RenderOptions) -> Result<Image> {
        let (width, height) = self.canvas()?;
        stage("render", || {
            on_pool(|| nakkash_raster::render_with(list, width, height, options))
        })
    }

    /// Hands the float canvas's work of every render after this to
    /// `accelerator`, or, with `None`, to nobody: renders draw on the CPU,
    /// the reference ([D74]). With one, frames approximate the reference
    /// within the tolerances D74 sets; `diff` and every check still draw on
    /// the CPU.
    ///
    /// [D74]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d74-an-opt-in-gpu-path-approximates-the-cpu-reference
    pub fn set_accelerator(&mut self, accelerator: Option<Arc<dyn Accelerator>>) {
        self.accelerator = accelerator;
    }

    /// What the accelerator did over this session's renders so far, if
    /// renders have one.
    pub fn tally(&self) -> Option<Tally> {
        self.accelerator.as_ref().map(|a| a.tally())
    }

    /// Whether any layer at any depth plays sound.
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub fn plays_sound(&self) -> Result<bool> {
        Ok(!nakkash_scene::routes(self.graph()?).is_empty())
    }

    /// The scene's sound over `frames`, possibly fractional: every audio
    /// layer mixed into one stereo track at [`nakkash_audio::MIX_RATE`]
    /// (steps `sound` for each file first played, and `mix`). Sound files
    /// are relative to the scene file's folder, and each is read once per
    /// edit.
    ///
    /// # Errors
    /// As [`Session::graph`], or a sound file that cannot be read or is in
    /// no format there is a decoder for, at its layer's `audio`.
    pub fn mix(&self, frames: Range<f64>) -> Result<nakkash_audio::Sound> {
        let graph = self.graph()?;
        let routes = nakkash_scene::routes(graph);
        let base = self.base();
        let sound = |route: &nakkash_scene::Route| {
            self.sounds.get(&base.join(route.path())).map_err(|e| {
                e.at(nakkash_format::key::AUDIO)
                    .under(nakkash_format::Place::of_layer(route.id(graph)))
            })
        };
        // Files are read before the mix is timed.
        for route in &routes {
            sound(route)?;
        }
        stage("mix", || crate::mix::mix(graph, &routes, frames, sound))
    }

    /// The whole frames the scene lasts: from 0 to its length, rounded up.
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub fn frames(&self) -> Result<Range<u32>> {
        Ok(0..self.graph()?.root.duration_frames.ceil() as u32)
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
        self.render_each(frames, options, |frame, drawn| {
            drawn
                .and_then(|image| each(frame, image))
                .map_err(|d| (frame, d))
        })
    }

    /// Draws every frame of `frames` as [`Session::render_frames`] does, and
    /// hands each image to `each` in frame order, one at a time: how frames
    /// reach an encoder. A thread that draws a frame too far ahead waits, so
    /// at most two frames per thread are held however long the range.
    ///
    /// # Errors
    /// As [`Session::render_frames`]. Once one fails, no frame after it is
    /// handed over.
    pub fn render_in_order(
        &self,
        frames: Range<u32>,
        options: RenderOptions,
        each: impl FnMut(u32, Image) -> Result<()> + Send,
    ) -> Result<()> {
        let window = 2 * workers(frames.len());
        let order = Mutex::new(InOrder {
            next: frames.start,
            waiting: BTreeMap::new(),
            failed: false,
            each,
        });
        let turn = Condvar::new();
        self.render_each(frames, options, |frame, drawn| {
            let mut order = order.lock().unwrap_or_else(PoisonError::into_inner);
            // The next frame is being drawn by a thread that does not wait,
            // so this wait ends, unless that frame fails.
            while frame >= order.next + window && !order.failed {
                order = turn.wait(order).unwrap_or_else(PoisonError::into_inner);
            }
            if order.failed {
                // The failure that stopped the render is the one reported.
                return Ok(());
            }
            // A frame handed over may be another thread's, so the failure
            // names its own frame.
            let handed = drawn
                .map_err(|d| (frame, d))
                .and_then(|image| order.hand(frame, image));
            if handed.is_err() {
                order.failed = true;
            }
            turn.notify_all();
            handed
        })
    }

    /// The worker loop of both renders: `each` gets every frame's image, or
    /// the failure to draw it, and fails with the frame that failed.
    fn render_each(
        &self,
        frames: Range<u32>,
        options: RenderOptions,
        each: impl Fn(u32, Result<Image>) -> std::result::Result<(), (u32, Diagnostic)> + Sync,
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
        // A frame whose list was drawn before in this render is copied
        // (D90).
        let (width, height) = self.canvas()?;
        let bytes = u64::from(width) * u64::from(height) * 4;
        let held = crate::held::Held::new(bytes, workers(frames.len()) as usize);
        self.on_threads(frames, |frame| {
            let drawn = self.display_list(f64::from(frame)).and_then(|list| {
                let id = nakkash_raster::frame_identity(&list, width, height, options);
                held.image(id, || self.draw(&list, options))
            });
            each(frame, drawn)
        })
    }

    /// Runs `work` on every frame of `frames`, one per processor thread at
    /// a time, in no set order: as [`Session::render_frames`] fails.
    pub(crate) fn on_threads(
        &self,
        frames: Range<u32>,
        work: impl Fn(u32) -> std::result::Result<(), (u32, Diagnostic)> + Sync,
    ) -> Result<()> {
        let next = AtomicU32::new(frames.start);
        let stop = AtomicBool::new(false);
        let failure: Mutex<Option<(u32, Diagnostic)>> = Mutex::new(None);
        // Frames being drawn, the signal a thread waiting to start one waits
        // for, and what one frame's grids came to, once a frame was drawn
        // alone.
        let drawing = AtomicUsize::new(0);
        let ended = (Mutex::new(()), Condvar::new());
        let per_frame = AtomicU64::new(0);
        // What the process held before any frame, and how much more than
        // its grids a frame drawn alone took: the rasteriser's own buffers,
        // pixmaps and masks.
        let base = nakkash_machine::resident().unwrap_or(0);
        let beyond_grids = AtomicU64::new(0);
        let starts = AtomicU64::new(0);
        let worker = || {
            let mut waited = false;
            while !stop.load(Ordering::Relaxed) {
                // A frame starts beside others only while what they may
                // hold fits the memory budget; else once they end, so
                // frames go one at a time, each on every thread, with the
                // same pixels (D90).
                let beside = drawing.load(Ordering::Acquire);
                if beside > 0 && !room_for(beside, base, per_frame.load(Ordering::Relaxed)) {
                    if !waited {
                        nakkash_raster::memory::note_held_back();
                        waited = true;
                    }
                    // Work the rows of the frames being drawn meanwhile.
                    if rayon::yield_now() != Some(rayon::Yield::Executed) {
                        let guard = ended.0.lock().unwrap_or_else(PoisonError::into_inner);
                        let _ = ended
                            .1
                            .wait_timeout(guard, std::time::Duration::from_millis(5));
                    }
                    continue;
                }
                // The frame's place beside the others is taken only if no
                // other thread took one since they were counted: else two
                // threads that both saw `beside` frames would start beside
                // each other, past what the budget let in.
                if drawing
                    .compare_exchange(beside, beside + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    continue;
                }
                waited = false;
                let frame = next.fetch_add(1, Ordering::Relaxed);
                if frame >= frames.end {
                    drawing.fetch_sub(1, Ordering::AcqRel);
                    ended.1.notify_all();
                    break;
                }
                nakkash_raster::memory::note_frames_at_once(beside as u64 + 1);
                let first = beside == 0;
                let started = starts.fetch_add(1, Ordering::AcqRel);
                nakkash_raster::memory::frame_start();
                let done = work(frame);
                // Alone: nothing was drawn beside it, from start to end.
                let alone = first && starts.load(Ordering::Acquire) == started + 1;
                drawing.fetch_sub(1, Ordering::AcqRel);
                // A frame's cost: the most grids it held at once, and, as
                // measured on a frame drawn alone, what it held beyond them
                // (the peak above what the process held before, less its
                // grids). The most of any frame stands for the next.
                let grids = nakkash_raster::memory::frame_peak();
                if alone {
                    let peak = nakkash_machine::usage().peak_resident;
                    beyond_grids.fetch_max(
                        peak.saturating_sub(base).saturating_sub(grids),
                        Ordering::Relaxed,
                    );
                }
                let cost = grids + beyond_grids.load(Ordering::Relaxed);
                per_frame.fetch_max(cost.max(1), Ordering::Relaxed);
                ended.1.notify_all();
                if let Err((frame, mut d)) = done {
                    stop.store(true, Ordering::Relaxed);
                    d.message = format!("frame {frame}: {}", d.message);
                    let mut first = failure.lock().unwrap_or_else(PoisonError::into_inner);
                    if first.as_ref().is_none_or(|(at, _)| frame < *at) {
                        *first = Some((frame, d));
                    }
                }
            }
        };
        // Every thread of the pool takes frames until none is left, then
        // works the rows of frames still being drawn. The frames are not
        // tasks of the pool: a thread waiting for a frame's rows takes only
        // rows, never a frame that would sit on top of the one it is in.
        match pool() {
            Some(pool) if pool.current_thread_index().is_none() => {
                pool.broadcast(|_| worker());
            }
            // Already drawing on the pool, or without one: this thread
            // takes every frame.
            _ => worker(),
        }
        match failure.into_inner().unwrap_or_else(PoisonError::into_inner) {
            Some((_, d)) => Err(d),
            None => Ok(()),
        }
    }

    /// The check report over every frame (step `check`), with frames read
    /// on one thread per processor, or as many as [`limit_threads`] allows.
    /// It is made once per edit; after an edit, from the check before it,
    /// reading again only what the edit can change ([D79]).
    ///
    /// [D79]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d79-an-edit-checks-only-what-it-can-change
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub fn check(&self) -> Result<Report> {
        Ok(self.checked()?.report().clone())
    }

    /// The scene's check, kept: what [`Session::check`] reports, and what
    /// the check after an edit starts from.
    ///
    /// # Errors
    /// As [`Session::graph`].
    pub fn checked(&self) -> Result<&Arc<Checked>> {
        let graph = self.graph()?;
        let scene = self.doc.scene();
        let threads = NonZero::new(workers(usize::MAX) as usize).unwrap_or(NonZero::<usize>::MIN);
        Ok(self.checked.get_or_init(|| {
            stage_ok("check", || {
                Arc::new(match &self.before {
                    Some(before) => before.checked.after(
                        (before.doc.scene(), &before.graph),
                        (scene, graph),
                        threads,
                    ),
                    None => Checked::new(scene, graph, threads),
                })
            })
        }))
    }

    /// The report of the pixel gates ([D59]): `contrast` and `flash`, on
    /// the scene drawn at a quarter of its size (step `gates`). They draw
    /// every frame, so they take a render's time where [`Session::check`]
    /// takes milliseconds.
    ///
    /// [D59]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d59-craft-lint-measures-motion-from-the-scene-and-reports-numbers
    ///
    /// # Errors
    /// As [`Session::render_frames`].
    pub fn gates(&self) -> Result<Report> {
        stage("gates", || crate::gates::gates(self))
    }

    /// The whole report `nakkash check` prints: the scene's rules, then its
    /// pixel gates ([`Session::check`], [`Session::gates`]). What a variant
    /// is measured by against its master ([D78]).
    ///
    /// [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error
    ///
    /// # Errors
    /// As [`Session::check`] and [`Session::gates`].
    pub fn full_check(&self) -> Result<Report> {
        let mut report = self.check()?;
        report.join(self.gates()?);
        Ok(report)
    }

    /// Where the layers in `ids` (all when empty) are at `time`, such as
    /// `1.5s` or `45f` (steps `time` and `at`).
    ///
    /// # Errors
    /// A diagnostic for a bad or negative time, or an id that is not a layer.
    pub fn at(&self, time: &str, ids: &[String]) -> Result<String> {
        let fps = self.doc.scene().root.fps;
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
        self.apply(line)?;
        self.check()
    }

    /// Applies the edit `line` (step `edit`) without checking the result.
    /// The version before it is kept when it was checked, so the next
    /// check starts from it.
    ///
    /// # Errors
    /// As [`Session::edit`]; the session is then left as it was.
    pub(crate) fn apply(&mut self, line: &str) -> Result<()> {
        let doc = stage("edit", || nakkash_edit::edit_line(&self.doc, line))?;
        self.swap(doc);
        Ok(())
    }

    /// Replaces the scene with `raw`, its JSON, once it validates (step
    /// `edit`), as an edit does: for tools that change the JSON themselves,
    /// such as the bake writing its digests. Nothing is written until
    /// [`save`](Session::save).
    ///
    /// # Errors
    /// The first problem in `raw`; the session is then left as it was.
    pub fn replace(&mut self, raw: serde_json::Value) -> Result<()> {
        let doc = stage("edit", || Document::from_value(raw))?;
        self.swap(doc);
        Ok(())
    }

    /// Takes `doc` as the scene, keeping the version before it when it was
    /// checked, so the next check starts from it, and dropping what was
    /// read for the one before.
    fn swap(&mut self, doc: Document) {
        let doc = std::mem::replace(&mut self.doc, doc);
        let graph = std::mem::take(&mut self.graph).into_inner();
        let checked = std::mem::take(&mut self.checked).into_inner();
        if let (Some(Ok(graph)), Some(checked)) = (graph, checked) {
            self.before = Some(Arc::new(Before {
                doc,
                graph,
                checked,
            }));
        }
        self.fonts = OnceLock::new();
        self.pictures = Pictures::default();
        self.clips = Clips::default();
        self.sounds = Sounds::default();
    }

    /// Applies one edit line of a variant ([D52]), as [`Session::edit`]
    /// does, except that a variant cannot edit the scene's `roles` or
    /// `locks`: those are the designer's.
    ///
    /// # Errors
    /// As [`Session::edit`], or a conflict diagnostic for an edit of
    /// `roles` or `locks`.
    ///
    /// [D52]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d52-a-batch-makes-every-variant-from-one-master
    pub fn edit_variant(&mut self, line: &str) -> Result<Report> {
        let parsed = nakkash_edit::parse_edit(&nakkash_edit::resolve_role(&self.doc, line)?)?;
        let designers = [nakkash_format::key::ROLES, nakkash_format::key::LOCKS];
        if let Some(change) = parsed
            .changes
            .iter()
            .find(|c| parsed.target.is_scene() && designers.contains(&c.key.first()))
        {
            return Err(Diagnostic::new(
                Kind::Conflict,
                format!(
                    "a variant cannot edit `{}`, which is the designer's",
                    change.key.first()
                ),
            )
            .expected("edits of the layers, slots and roles' values"));
        }
        self.edit(line)
    }

    /// A copy of this session with the layers at `addresses` hidden, as an
    /// edit hides them, so one inside a composition is hidden in that
    /// instance alone; this one is left as it is.
    ///
    /// # Errors
    /// As [`Session::edit`], for a layer the scene does not have.
    pub(crate) fn hiding(&self, addresses: &[Address]) -> Result<Session> {
        let mut hidden = self.variant();
        for address in addresses {
            hidden.apply(&format!("{address} hidden=true"))?;
        }
        Ok(hidden)
    }

    /// A copy of this session to make a variant of ([D52]): the same scene
    /// and file, sharing the compiled scene and its check, with nothing
    /// drawn yet. Edits of the copy leave this one as it is, and the copy's
    /// checks start from this one's ([D79]).
    ///
    /// [D79]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d79-an-edit-checks-only-what-it-can-change
    ///
    /// [D52]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d52-a-batch-makes-every-variant-from-one-master
    pub fn variant(&self) -> Session {
        Session {
            path: self.path.clone(),
            instance: self.instance.clone(),
            doc: self.doc.clone(),
            fonts: OnceLock::new(),
            graph: shared(&self.graph),
            checked: shared(&self.checked),
            before: self.before.clone(),
            font_cache: self.font_cache.clone(),
            pictures: Pictures::default(),
            clips: Clips::default(),
            sounds: Sounds::default(),
            accelerator: self.accelerator.clone(),
        }
    }

    /// A session on `raw`, another scene's JSON, as if read from this
    /// session's file: the files it names resolve from the same folder, and
    /// it loads fonts from the same cache. What a generated variant is made
    /// in ([D78]).
    ///
    /// [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error
    ///
    /// # Errors
    /// The first problem in `raw` as a scene.
    pub fn with_raw(&self, raw: serde_json::Value) -> Result<Session> {
        let doc = stage("parse", || Document::from_value(raw))?;
        Ok(Session {
            path: self.path.clone(),
            instance: self.instance.clone(),
            doc,
            fonts: OnceLock::new(),
            graph: OnceLock::new(),
            checked: OnceLock::new(),
            before: None,
            font_cache: self.font_cache.clone(),
            pictures: Pictures::default(),
            clips: Clips::default(),
            sounds: Sounds::default(),
            accelerator: self.accelerator.clone(),
        })
    }

    /// The validated document: the scene and the JSON it was read from.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Writes the scene, in canonical form, to `path` instead of its own
    /// file, with the files it names (its `fonts`, and each layer's `image`
    /// and `video`) written relative to `path`'s folder, so it reads them
    /// from there as it did from its own.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be written.
    pub fn save_as(&self, path: &Path) -> Result<()> {
        let mut raw = self.doc.raw().clone();
        let from = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(Path::new(""));
        let to = path.parent().unwrap_or(Path::new(""));
        crate::relocate::files(&mut raw, from, to);
        let tmp = temp_path(path);
        fs::write(&tmp, write_document(&raw)).map_err(|e| Diagnostic::io("write", path, &e))?;
        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            Diagnostic::io("write", path, &e)
        })
    }

    /// Writes the scene split ([D91]): the scene to `path`, with each
    /// composition's entry an include of `comps/<id>.json`, and each
    /// composition to that file beside it, every file it names written
    /// relative to `path`'s folder. Opened again, it reads as this scene.
    ///
    /// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when a file or the folder cannot be written.
    pub fn save_split(&self, path: &Path) -> Result<()> {
        let mut raw = self.doc.raw().clone();
        let from = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(Path::new(""));
        let to = path.parent().unwrap_or(Path::new(""));
        crate::relocate::files(&mut raw, from, to);
        let (top, comps) = nakkash_format::include::split(&raw);
        for (file, comp) in &comps {
            let file = to.join(file);
            if let Some(folder) = file.parent() {
                fs::create_dir_all(folder).map_err(|e| Diagnostic::io("create", folder, &e))?;
            }
            fs::write(&file, write_document(comp))
                .map_err(|e| Diagnostic::io("write", &file, &e))?;
        }
        fs::write(path, write_document(&top)).map_err(|e| Diagnostic::io("write", path, &e))
    }

    /// Writes the scene, in canonical form, back to its file (step `save`),
    /// through a renamed temp file so a failed write never leaves half a scene.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be written, a
    /// [`Kind::Missing`] one when the session has no file, or a
    /// [`Kind::Conflict`] one when its scene was merged from an instance,
    /// whose template is locked.
    pub fn save(&self) -> Result<()> {
        stage("save", || {
            if let Some(instance) = &self.instance {
                return Err(Diagnostic::new(
                    Kind::Conflict,
                    format!(
                        "the scene of the instance `{}` is its template, which is locked",
                        instance.display()
                    ),
                )
                .expected("an edit of the instance: `nakkash edit <instance> '$id=value'`"));
            }
            let path = self.path.as_deref().ok_or_else(|| {
                Diagnostic::new(Kind::Missing, "the scene has no file to save to")
                    .expected("a session from `Session::open`")
            })?;
            if crate::includes::is_split(path) {
                return Err(crate::includes::refused(path));
            }
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
        for note in doc.deprecations() {
            tracing::warn!(target: nakkash_format::deprecated::TARGET, "{note}");
        }
        Ok(Self {
            path: None,
            instance: None,
            doc,
            fonts: OnceLock::new(),
            graph: OnceLock::new(),
            checked: OnceLock::new(),
            before: None,
            font_cache: None,
            pictures: Pictures::default(),
            clips: Clips::default(),
            sounds: Sounds::default(),
            accelerator: None,
        })
    }
}

/// A version of the scene that was checked, kept after an edit.
#[derive(Debug)]
struct Before {
    /// The scene as it was.
    doc: Document,
    /// Its compiled form.
    graph: Arc<SceneGraph>,
    /// Its check.
    checked: Arc<Checked>,
}

/// A copy of `cell` that shares what it holds, if anything.
fn shared<T: Clone>(cell: &OnceLock<T>) -> OnceLock<T> {
    let copy = OnceLock::new();
    if let Some(held) = cell.get() {
        let _ = copy.set(held.clone());
    }
    copy
}

/// Whether a `fonts` entry is a lock.
fn is_lock(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e == crate::fonts::LOCK_EXTENSION)
}

/// A canvas side in whole pixels. Validation keeps sizes whole and
/// positive, and the cast saturates, so a side is never wrapped.
fn pixels(side: f64) -> u32 {
    side.round() as u32
}

/// The sibling file the new text is written to before it replaces `path`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Works on at most `threads` threads from now on, in this process: the
/// frames of a render and the rows of each frame share them ([D75]), and
/// checks, comparisons and the encoder keep to them too
/// ([`nakkash_machine::set_thread_limit`]). Each
/// thread holds the frame it draws, so fewer threads take less memory: on
/// a machine shared with other work, or for scenes whose frames are large
/// (3D planes seen up close).
///
/// [D75]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d75-a-frame-is-drawn-on-every-thread-of-its-render
pub fn limit_threads(threads: NonZero<usize>) {
    nakkash_machine::set_thread_limit(Some(threads));
}

/// How many threads draw: the limit set, or the machine's default (its
/// performance cores on macOS, the processors the process may use
/// elsewhere, [`nakkash_machine::default_threads`]).
pub(crate) fn threads() -> usize {
    nakkash_machine::threads()
}

/// Whether a frame may start beside `beside` frames being drawn: the
/// budget is not held, and what the process held before the render, `base`,
/// with `per_frame` bytes for each of them and the new one, fits it. Before
/// a frame was drawn alone `per_frame` is 0, unknown, and frames start one
/// at a time.
fn room_for(beside: usize, base: u64, per_frame: u64) -> bool {
    let budget = nakkash_raster::memory::stats().effective;
    // A quarter more than measured: frames differ, and one measured is a
    // floor for the next.
    let per_frame = per_frame.saturating_add(per_frame / 4);
    let planned = base.saturating_add(per_frame.saturating_mul(beside as u64 + 1));
    // What is resident now holds the frames being drawn as far as they
    // got, and the pool: the new frame must fit beside it too.
    let now = nakkash_raster::memory::resident()
        .unwrap_or(0)
        .saturating_add(per_frame);
    per_frame > 4 && planned.max(now) <= budget && !nakkash_raster::memory::over()
}

/// Whether [`limit_memory`] set the budget.
static MEMORY_SET: AtomicBool = AtomicBool::new(false);

/// Holds what renders keep, pool and work to `bytes` from now on, in this
/// process ([D90]): over it, a render lets kept drawings and pooled grids
/// go and draws frames one at a time, and draws the same pixels. Without
/// it, the budget is a quarter of the machine's memory, at most 4 GiB
/// ([`nakkash_machine::default_budget`]).
///
/// [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget
pub fn limit_memory(bytes: u64) {
    MEMORY_SET.store(true, Ordering::Relaxed);
    nakkash_raster::memory::set_budget(bytes);
}

/// The machine's memory budget, unless [`limit_memory`] set one, and the
/// memory-pressure probe: set once, before the first frame is drawn.
fn memory_ready() {
    static READY: std::sync::Once = std::sync::Once::new();
    READY.call_once(|| {
        if !MEMORY_SET.load(Ordering::Relaxed) {
            nakkash_raster::memory::set_budget(nakkash_machine::default_budget());
        }
        nakkash_raster::memory::set_probe(reading);
        nakkash_raster::memory::set_resident_probe(nakkash_machine::resident);
    });
}

/// The machine's memory pressure and what this process holds resident, as
/// the rasteriser's budget reads them.
fn reading() -> nakkash_raster::memory::Reading {
    use nakkash_raster::memory::Pressure;
    nakkash_raster::memory::Reading {
        pressure: match nakkash_machine::pressure() {
            nakkash_machine::Pressure::Normal => Pressure::Normal,
            nakkash_machine::Pressure::Warning => Pressure::Warning,
            nakkash_machine::Pressure::Critical => Pressure::Critical,
        },
        resident: nakkash_machine::resident(),
    }
}

/// How many threads take frames of a render of `frames` frames: one per
/// processor, or the limit set, no more than there are frames.
fn workers(frames: usize) -> u32 {
    threads_for(
        frames,
        nakkash_machine::default_threads(),
        nakkash_machine::thread_limit().map_or(0, NonZero::get),
    )
}

/// The room on each thread's stack: a frame's layers are drawn by
/// recursion, and the rows of another frame worked while one waits sit on
/// top of it.
const STACK: usize = 8 << 20;

/// The pool every frame is drawn on, with [`threads`] threads: made the
/// first time it is needed, and again when the limit changes. `None` where
/// threads cannot be started, such as WebAssembly without them; a frame is
/// then drawn on the thread that asks for it.
fn pool() -> Option<Arc<ThreadPool>> {
    static POOL: Mutex<Option<(usize, Arc<ThreadPool>)>> = Mutex::new(None);
    memory_ready();
    let threads = threads();
    let mut pool = POOL.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((made, pool)) = &*pool
        && *made == threads
    {
        return Some(Arc::clone(pool));
    }
    let made = ThreadPoolBuilder::new()
        .num_threads(threads)
        .stack_size(STACK)
        .thread_name(|i| format!("nakkash-draw-{i}"))
        .build()
        .ok()
        .map(Arc::new)?;
    *pool = Some((threads, Arc::clone(&made)));
    Some(made)
}

/// Runs `work` on the drawing pool, so the rows it splits share the
/// render's threads: on this thread when it is one of them.
pub(crate) fn on_pool<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    match pool() {
        Some(pool) if pool.current_thread_index().is_none() => pool.install(work),
        _ => work(),
    }
}

/// How many threads draw `frames` frames on `processors`, at most `limit`
/// unless it is 0.
fn threads_for(frames: usize, processors: usize, limit: usize) -> u32 {
    let threads = match limit {
        0 => processors,
        limit => limit,
    };
    threads.min(frames.max(1)) as u32
}

/// The frames of [`Session::render_in_order`] drawn ahead of their turn.
struct InOrder<F> {
    /// The frame to hand over next.
    next: u32,
    waiting: BTreeMap<u32, Image>,
    /// Whether a frame failed, so none after it is handed over.
    failed: bool,
    each: F,
}

impl<F: FnMut(u32, Image) -> Result<()>> InOrder<F> {
    /// Takes `frame`'s image, and hands over every frame whose turn it is.
    /// A failure names the frame that failed.
    fn hand(&mut self, frame: u32, image: Image) -> std::result::Result<(), (u32, Diagnostic)> {
        self.waiting.insert(frame, image);
        while let Some(image) = self.waiting.remove(&self.next) {
            let at = self.next;
            self.next += 1;
            (self.each)(at, image).map_err(|d| (at, d))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/session.rs"]
mod tests;
