//! `nakkash ml preview`, `bake` and `track`: the requests a scene's video
//! layers make of a bake ([D85]), run on their clips, and what comes back
//! written into the scene.
//!
//! A request is a layer's `cutout` or one of its `tracks`. Every request of
//! the scene is run, or those of the one layer `--layer` names; a layer
//! shown by several composition layers is run once. The segmentation of a
//! set of prompts is kept in the work cache, so a preview followed by a
//! bake segments once, and a cutout and a track with the same prompts
//! share one.
//!
//! [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use serde_json::Value;

use nakkash_baked::{Manifest, Matte, Store, Track};
use nakkash_engine::{Result, Session};
use nakkash_format::key::{COMPS, CUTOUT, ID, LAYERS, TRACKS};
use nakkash_format::{
    Address, Content, Cutout, Diagnostic, Kind, Layer, Sha256, Step, TrackRequest, VideoContent,
};
use nakkash_ml::bake::{self, Baker, TrackBake};
use nakkash_ml::image::Rgb8;
use nakkash_ml::segment::{Frames, Prompt};
use nakkash_ml::track::{self, Mask, PivotRequest, PointTracker, TrackSpec};
use nakkash_ml::{ModelLock, ModelStore, matte, preview};
use nakkash_onnx::Onnx;
use nakkash_video::Clip;

/// The key a request keeps its bake's digest under.
const BAKED: &str = "baked";

/// A video layer that asks a bake for something.
struct Asking<'s> {
    /// Where it is: the first address that reaches it.
    address: Address,
    /// The composition whose list holds it, or `None` for the scene's.
    list: Option<String>,
    layer: &'s Layer,
    video: &'s VideoContent,
    /// Its clip, as the render opens it.
    clip: PathBuf,
}

/// The video layers of `session` that ask for a cutout or a track, or the
/// one `only` addresses.
///
/// # Errors
/// A [`Kind::Missing`] diagnostic when `only` names no such layer.
fn asking<'s>(session: &'s Session, only: Option<&Address>) -> Result<Vec<Asking<'s>>> {
    let scene = session.scene();
    let base = session
        .path()
        .and_then(Path::parent)
        .unwrap_or(Path::new(""))
        .to_path_buf();
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    let mut queue: VecDeque<(Vec<nakkash_format::Id>, Option<String>, &[Layer])> =
        VecDeque::from([(Vec::new(), None, scene.root.layers.as_slice())]);
    while let Some((path, list, layers)) = queue.pop_front() {
        for layer in layers {
            let mut here = path.clone();
            here.push(layer.id.parse()?);
            match &layer.content {
                Content::Comp(comp) => {
                    if let Some(inner) = scene.comps.iter().find(|c| c.id == comp.id) {
                        queue.push_back((here, Some(comp.id.to_string()), &inner.layers));
                    }
                }
                Content::Video(video) if video.cutout.is_some() || !video.tracks.is_empty() => {
                    let Some(file) = &video.path else { continue };
                    if !seen.insert((list.clone(), layer.id.to_string())) {
                        continue;
                    }
                    found.push(Asking {
                        address: Address {
                            layers: here,
                            keys: Vec::new(),
                        },
                        list: list.clone(),
                        layer,
                        video,
                        clip: base.join(file),
                    });
                }
                _ => {}
            }
        }
    }
    found.sort_by(|a, b| a.address.cmp(&b.address));
    match only {
        None => Ok(found),
        Some(only) => {
            let one: Vec<Asking> = found
                .into_iter()
                .filter(|a| a.address.layers == only.layers)
                .collect();
            match one.is_empty() {
                true => Err(Diagnostic::new(
                    Kind::Missing,
                    format!("`{only}` is not a video layer with a cutout or tracks"),
                )
                .expected("the address of a video layer whose clip has a `cutout` or `tracks`")),
                false => Ok(one),
            }
        }
    }
}

/// A clip read frame by frame for the models, in the order frames show.
struct ClipFrames(Arc<Clip>);

impl Frames for ClipFrames {
    fn len(&self) -> usize {
        self.0.frames()
    }

    fn size(&self) -> (usize, usize) {
        let (w, h) = self.0.size();
        (w as usize, h as usize)
    }

    fn frame(&mut self, index: usize) -> nakkash_format::Result<Rgb8> {
        let frame = self.0.frame(index)?;
        Ok(Rgb8 {
            width: frame.width as usize,
            height: frame.height as usize,
            data: frame
                .pixels
                .chunks_exact(4)
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect(),
        })
    }
}

/// An open clip and its hash.
struct Opened {
    clip: Arc<Clip>,
    hash: Sha256,
}

impl Opened {
    fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            clip: Arc::new(Clip::open(path)?),
            hash: Sha256::of_file(path)?.0,
        })
    }

    fn frames(&self) -> ClipFrames {
        ClipFrames(Arc::clone(&self.clip))
    }

    /// When each frame starts to show, in seconds into the clip.
    fn times(&self) -> Vec<f64> {
        (0..self.clip.frames())
            .map(|f| self.clip.start(f))
            .collect()
    }
}

/// A scene's prompts as the models take them.
fn prompts(prompts: &[nakkash_format::Prompt]) -> Vec<Prompt> {
    let point = |p: &[f64; 2]| [p[0] as f32, p[1] as f32];
    prompts
        .iter()
        .map(|p| Prompt {
            frame: p.frame as usize,
            area: p.bbox.map(|b| b.map(|v| v as f32)),
            include: p.include.iter().map(point).collect(),
            exclude: p.exclude.iter().map(point).collect(),
        })
        .collect()
}

/// A scene's cutout as the bake takes it.
fn cutout(cutout: &Cutout) -> bake::Cutout {
    bake::Cutout {
        prompts: prompts(&cutout.prompts),
        edges: match cutout.edges {
            nakkash_format::Edges::Refine => matte::Edges::Refine,
            nakkash_format::Edges::Hard => matte::Edges::Hard,
            nakkash_format::Edges::Soft => matte::Edges::Soft,
        },
        band: cutout.band as f32,
        agree: cutout.agree as f32,
        smooth: cutout.smooth.map(|s| (s.amount as f32, s.tolerance as f32)),
        request: cutout.request(),
    }
}

/// A scene's track as the bake takes it.
fn track_bake(request: &TrackRequest) -> TrackBake {
    TrackBake {
        prompts: prompts(&request.prompts),
        spec: TrackSpec {
            prompt_frame: request.first_frame(),
            corners: request.corners,
            anchors: request
                .anchors
                .iter()
                .map(|a| track::Anchor {
                    frame: a.frame,
                    corners: a.corners,
                })
                .collect(),
        },
        request: request.request(),
    }
}

/// What the models run on: the store, the lock, the runtime.
pub struct Models {
    store: ModelStore,
    lock: ModelLock,
    backend: Onnx,
    threads: usize,
}

impl Models {
    /// The runtime loaded from `store`, graphs on `threads` threads.
    pub fn load(store: ModelStore, lock: ModelLock, threads: usize) -> Result<Self> {
        let backend = Onnx::load(&store, &lock)?;
        Ok(Self {
            store,
            lock,
            backend,
            threads,
        })
    }

    fn baker<'a>(&'a self, baked: &'a Store) -> Baker<'a> {
        Baker::new(
            &self.backend,
            &self.store,
            &self.lock,
            baked,
            self.threads,
            |line| eprintln!("  ml: {line}"),
        )
    }
}

/// `nakkash ml preview`: a contact sheet for each request, of `count`
/// frames `tile` px wide, written at `out`, or beside it, one per request,
/// when there are several.
pub fn preview(
    models: &Models,
    scene: &Path,
    only: Option<&Address>,
    out: &Path,
    count: usize,
    tile: usize,
) -> Result<ExitCode> {
    let session = Session::open(scene)?;
    let baked = session.baked_store()?;
    let baker = models.baker(&baked);
    let asking = asking(&session, only)?;
    let mut sheets: Vec<(String, Vec<Prompt>, bool)> = Vec::new();
    let mut runs = Vec::new();
    for a in &asking {
        sheets.clear();
        if let Some(c) = &a.video.cutout {
            sheets.push((format!("{}-cutout", a.layer.id), prompts(&c.prompts), false));
        }
        for t in &a.video.tracks {
            sheets.push((
                format!("{}-{}", a.layer.id, t.id),
                prompts(&t.prompts),
                true,
            ));
        }
        runs.push((a, sheets.clone()));
    }
    let total: usize = runs.iter().map(|(_, s)| s.len()).sum();
    for (a, sheets) in runs {
        let opened = Opened::new(&a.clip)?;
        for (name, prompts, points) in sheets {
            let mut frames = opened.frames();
            let mut made = baker.made();
            let (logits, _) = baker.segmentation(&mut frames, &prompts, &opened.hash, &mut made)?;
            let followed = match points {
                false => Vec::new(),
                true => follow(models, &opened, &logits, &prompts)?,
            };
            let (png, shown) =
                preview::sheet(&mut frames, &logits, &prompts, &followed, count, tile)?;
            let path = match total {
                1 => out.to_path_buf(),
                _ => beside(out, &name),
            };
            std::fs::write(&path, png).map_err(|e| Diagnostic::io("write", &path, &e))?;
            println!(
                "  {} {name}: frames {}",
                path.display(),
                shown
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The points a track would follow from its prompted frame, where each is
/// seen on each frame.
fn follow(
    models: &Models,
    opened: &Opened,
    logits: &[nakkash_ml::image::Plane],
    prompts: &[Prompt],
) -> Result<Vec<Vec<[f64; 2]>>> {
    let first = prompts.iter().map(|p| p.frame).min().unwrap_or(0);
    let (w, h) = opened.frames().size();
    let mask = Mask::from_logits(&logits[first], w, h);
    let chosen = track::points::choose(&opened.frames().frame(first)?, &mask);
    let mut tracker = PointTracker::open(
        &models.backend,
        &models.store,
        &models.lock,
        models.threads,
        Box::new(opened.frames()),
    )?;
    let tracks = track::follow_all(&mut tracker, first as u32, &chosen)?;
    Ok((0..logits.len())
        .map(|f| {
            tracks
                .iter()
                .filter(|t| t.seen[f])
                .map(|t| t.at[f])
                .collect()
        })
        .collect())
}

/// `out` with `-name` added to its stem.
fn beside(out: &Path, name: &str) -> PathBuf {
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("sheet");
    let ext = out.extension().and_then(|s| s.to_str()).unwrap_or("png");
    out.with_file_name(format!("{stem}-{name}.{ext}"))
}

/// `nakkash ml bake`: bakes every request whose bake is missing or stale,
/// or every one with `again`, and writes the digests into the scene.
pub fn bake(
    models: &Models,
    scene: &Path,
    only: Option<&Address>,
    again: bool,
) -> Result<ExitCode> {
    let mut session = Session::open(scene)?;
    let baked = session.baked_store()?;
    let baker = models.baker(&baked);
    let mut writes: Vec<(Option<String>, String, Option<String>, Sha256)> = Vec::new();
    for a in asking(&session, only)? {
        let opened = Opened::new(&a.clip)?;
        if let Some(c) = &a.video.cutout {
            let request = c.request();
            let fresh = c.baked.is_some_and(|d| {
                Manifest::<Matte>::open(&baked, &d, &request, &opened.hash).is_ok()
            });
            if fresh && !again {
                println!("  {} cutout: baked already", a.address);
            } else {
                let done = baker.cutout(&mut opened.frames(), &cutout(c), &opened.hash)?;
                println!(
                    "  {} cutout: {} frames, the edge refined on {}, {:.2} s a frame, baked {}",
                    a.address, done.frames, done.refined, done.seconds_per_frame, done.digest
                );
                writes.push((a.list.clone(), a.layer.id.to_string(), None, done.digest));
            }
        }
        for t in &a.video.tracks {
            let request = t.request();
            let fresh = t.baked.is_some_and(|d| {
                Manifest::<Track>::open(&baked, &d, &request, &opened.hash).is_ok()
            });
            if fresh && !again {
                println!("  {}.tracks[{}]: baked already", a.address, t.id);
                continue;
            }
            let done = baker.track(
                &mut opened.frames(),
                Box::new(opened.frames()),
                &opened.times(),
                &track_bake(t),
                &opened.hash,
            )?;
            println!(
                "  {}.tracks[{}]: {} frames, corners from the similarity on {}, {:.2} s a frame, baked {}",
                a.address, t.id, done.frames, done.refused, done.seconds_per_frame, done.digest
            );
            writes.push((
                a.list.clone(),
                a.layer.id.to_string(),
                Some(t.id.clone()),
                done.digest,
            ));
        }
    }
    if writes.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    let mut raw = session.document().raw().clone();
    for (list, layer, track, digest) in &writes {
        let object = layer_mut(&mut raw, list, layer)?;
        let target = match track {
            None => object.get_mut(CUTOUT),
            Some(id) => object
                .get_mut(TRACKS)
                .and_then(Value::as_array_mut)
                .and_then(|ts| {
                    ts.iter_mut()
                        .find(|t| t.get(ID).and_then(Value::as_str) == Some(id))
                }),
        };
        let Some(Value::Object(target)) = target else {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("the request of `{layer}` moved while it baked"),
            ));
        };
        target.insert(BAKED.into(), Value::String(digest.to_string()));
    }
    session.replace(raw)?;
    session.save()?;
    println!("  wrote {} digests into {}", writes.len(), scene.display());
    Ok(ExitCode::SUCCESS)
}

/// The JSON object of layer `id` in `list`, the scene's or a composition's.
fn layer_mut<'v>(raw: &'v mut Value, list: &Option<String>, id: &str) -> Result<&'v mut Value> {
    let holder = match list {
        None => Some(&mut *raw),
        Some(comp) => raw.get_mut(COMPS).and_then(|c| c.get_mut(comp)),
    };
    holder
        .and_then(|h| h.get_mut(LAYERS))
        .and_then(Value::as_array_mut)
        .and_then(|layers| {
            layers
                .iter_mut()
                .find(|l| l.get(ID).and_then(Value::as_str) == Some(id))
        })
        .ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!("no layer `{id}` in the scene's JSON"),
            )
        })
}

/// `nakkash ml track`: writes the baked track `address` names
/// (`<layer>.tracks[<id>]`) onto the pivot `pivot`, and its corners onto
/// four more with `corners`.
pub fn apply(scene: &Path, address: &Address, pivot: &str, corners: bool) -> Result<ExitCode> {
    let expected = "`<layer>.tracks[<id>]`, the address of a video layer's track";
    let [Step::Key(key), Step::Id(id)] = address.keys.as_slice() else {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("`{address}` names no track"))
                .expected(expected),
        );
    };
    if key != TRACKS {
        return Err(
            Diagnostic::new(Kind::Invalid, format!("`{address}` names no track"))
                .expected(expected),
        );
    }
    let video = Address {
        layers: address.layers.clone(),
        keys: Vec::new(),
    };
    let mut session = Session::open(scene)?;
    let found = asking(&session, Some(&video))?;
    let a = &found[0];
    let request = a
        .video
        .tracks
        .iter()
        .find(|t| t.id == id.as_ref())
        .ok_or_else(|| Diagnostic::new(Kind::Missing, format!("`{video}` has no track `{id}`")))?;
    let digest = request.baked.ok_or_else(|| {
        Diagnostic::new(
            Kind::Missing,
            format!(
                "the track `{id}` has not been baked; run `{}` on the scene",
                nakkash_baked::BAKE_COMMAND
            ),
        )
    })?;
    let hash = Sha256::of_file(&a.clip)?.0;
    let baked = session.baked_store()?;
    let track = Manifest::<Track>::open(&baked, &digest, &request.request(), &hash)?;
    let mut raw = session.document().raw().clone();
    let applied = track::apply(
        &mut raw,
        &PivotRequest {
            video,
            pivot: pivot.into(),
            corners,
        },
        &track,
    )?;
    session.replace(raw)?;
    session.save()?;
    for (pivot, keys) in &applied.pivots {
        let how = if applied.made.contains(pivot) {
            "made"
        } else {
            "updated"
        };
        println!("  {pivot}: {how}, {keys} keys");
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/ml/scene.rs"]
mod tests;
