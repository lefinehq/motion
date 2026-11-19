//! [`Baker`]: runs the stages of a bake and keeps what each makes ([D85]).
//!
//! A stage's result is kept in the work cache (`ml/work` of the nakkash
//! cache) under a key over everything that shapes it: the files of the
//! graphs it runs, the runtime, where each graph ran, the threads, its
//! settings, its prompts and the clip's hash. A preview's segmentation is
//! the bake's, and a track and a cutout with the same prompts share one.
//! The finished matte goes to the baked store as an asset.
//!
//! [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use nakkash_baked::{Alpha, Made, MatteWriter, Store};
use nakkash_format::{Diagnostic, Kind, Result, Sha256};

use crate::flow::{self, Flow};
use crate::image::Plane;
use crate::matte::{self, EdgeMatter, Edges, Mask};
use crate::segment::{self, Frames, Prompt, Segmenter};
use crate::{Backend, ModelLock, ModelStore};

/// What a cutout asks for, as a bake reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Cutout {
    /// The prompts.
    pub prompts: Vec<Prompt>,
    /// What the matte is made of.
    pub edges: Edges,
    /// The edge band's half-width in px.
    pub band: f32,
    /// The IoU at which the refinement counts.
    pub agree: f32,
    /// The smoothing's amount and tolerance, when asked for.
    pub smooth: Option<(f32, f32)>,
    /// The request as the scene writes it, which the asset answers.
    pub request: Value,
}

/// What a bake of a cutout did.
#[derive(Debug, Clone, PartialEq)]
pub struct Baked {
    /// The matte's digest.
    pub digest: Sha256,
    /// Frames whose edge the refinement took.
    pub refined: usize,
    /// Frames in the clip.
    pub frames: usize,
    /// Whether the segmentation came from the work cache.
    pub segmentation_kept: bool,
    /// Seconds a frame took, segmentation and refinement together.
    pub seconds_per_frame: f64,
}

/// The index of a kept stage: its frames, each a blob of little-endian
/// floats in the work store.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    width: usize,
    height: usize,
    frames: Vec<Sha256>,
}

/// Runs bakes on one machine's backend.
pub struct Baker<'a> {
    backend: &'a dyn Backend,
    store: &'a ModelStore,
    lock: &'a ModelLock,
    baked: &'a Store,
    work: Store,
    threads: usize,
    progress: Box<dyn Fn(&str) + 'a>,
}

impl std::fmt::Debug for Baker<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Baker")
            .field("threads", &self.threads)
            .finish_non_exhaustive()
    }
}

impl<'a> Baker<'a> {
    /// A baker running graphs from `store` on `backend` with `threads`
    /// threads each, writing assets to `baked`, telling `progress` how it
    /// goes.
    pub fn new(
        backend: &'a dyn Backend,
        store: &'a ModelStore,
        lock: &'a ModelLock,
        baked: &'a Store,
        threads: usize,
        progress: impl Fn(&str) + 'a,
    ) -> Self {
        Self {
            backend,
            store,
            lock,
            baked,
            work: Store::new(store.files().root().with_file_name("work")),
            threads,
            progress: Box::new(progress),
        }
    }

    /// A record of this machine and runtime, with nothing run yet.
    pub fn made(&self) -> Made {
        let mut made = Made::here(self.threads);
        made.runtime = self.backend.runtime();
        made
    }

    /// The key of a stage of `model` with `settings`, for the clip hashing
    /// to `clip`.
    fn key(&self, stage: &str, model: &str, settings: Value, clip: &Sha256) -> Result<Sha256> {
        let locked = self.lock.model(model)?;
        let graphs: Vec<Value> = locked
            .graphs
            .iter()
            .map(|(name, g)| json!([name, g.sha256, g.placement.here()]))
            .collect();
        let runtime = self.backend.runtime().map(|r| r.sha256);
        let key = json!({"stage": stage, "graphs": graphs, "runtime": runtime, "threads": self.threads,
                         "settings": settings, "clip": clip});
        Ok(Sha256::of(&serde_json::to_vec(&key).expect("a key writes")))
    }

    /// Where the index of the stage with `key` is kept.
    fn index(&self, key: &Sha256) -> PathBuf {
        self.work.root().join("keys").join(format!("{key}.json"))
    }

    /// The planes kept under `key`, if all are.
    fn kept(&self, key: &Sha256) -> Result<Option<Vec<Plane>>> {
        let Ok(text) = fs::read(self.index(key)) else {
            return Ok(None);
        };
        let Ok(kept) = serde_json::from_slice::<Kept>(&text) else {
            return Ok(None);
        };
        let mut planes = Vec::with_capacity(kept.frames.len());
        for hash in &kept.frames {
            let Some(bytes) = self.work.get(hash)? else {
                return Ok(None);
            };
            planes.push(Plane {
                width: kept.width,
                height: kept.height,
                data: bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
            });
        }
        Ok(Some(planes))
    }

    /// Keeps `planes` under `key`.
    fn keep(&self, key: &Sha256, planes: &[Plane]) -> Result<()> {
        let mut frames = Vec::with_capacity(planes.len());
        for plane in planes {
            let bytes: Vec<u8> = plane.data.iter().flat_map(|v| v.to_le_bytes()).collect();
            frames.push(self.work.put(&bytes)?);
        }
        let (width, height) = planes.first().map_or((0, 0), |p| (p.width, p.height));
        let index = self.index(key);
        let dir = index.parent().expect("a folder");
        fs::create_dir_all(dir).map_err(|e| Diagnostic::io("create", dir, &e))?;
        let text = serde_json::to_vec(&Kept {
            width,
            height,
            frames,
        })
        .expect("an index writes");
        fs::write(&index, text).map_err(|e| Diagnostic::io("write", &index, &e))
    }

    /// The segmentation of `frames` from `prompts`: 256² logits per frame,
    /// from the work cache when it has them, and whether it did.
    ///
    /// # Errors
    /// The segmenter's, the store's and the clip's diagnostics.
    pub fn segmentation(
        &self,
        frames: &mut dyn Frames,
        prompts: &[Prompt],
        clip: &Sha256,
        made: &mut Made,
    ) -> Result<(Vec<Plane>, bool)> {
        let settings: Vec<Value> = prompts
            .iter()
            .map(|p| json!({"frame": p.frame, "box": p.area, "include": p.include, "exclude": p.exclude}))
            .collect();
        let key = self.key("segment", segment::MODEL, json!(settings), clip)?;
        for (name, g) in &self.lock.model(segment::MODEL)?.graphs {
            made.ran(nakkash_baked::GraphRun {
                model: segment::MODEL.into(),
                graph: name.clone(),
                sha256: g.sha256,
                backend: g.placement.here().into(),
            });
        }
        if let Some(planes) = self.kept(&key)? {
            (self.progress)(&format!(
                "segmentation of {} frames kept from an earlier run",
                planes.len()
            ));
            return Ok((planes, true));
        }
        let mut segmenter = Segmenter::open(self.backend, self.store, self.lock, self.threads)?;
        let n = frames.len();
        let start = Instant::now();
        let mut done = 0;
        let planes = segmenter.segment(frames, prompts, |f| {
            done += 1;
            (self.progress)(&format!(
                "segmented frame {f} ({done} of {n}, {:.2} s a frame)",
                start.elapsed().as_secs_f64() / done as f64
            ));
        })?;
        self.keep(&key, &planes)?;
        Ok((planes, false))
    }

    /// Bakes `cutout` of the clip `frames`, hashing to `clip`, into a matte.
    ///
    /// # Errors
    /// The stages' diagnostics, and the store's when the matte cannot be
    /// written.
    pub fn cutout(&self, frames: &mut dyn Frames, cutout: &Cutout, clip: &Sha256) -> Result<Baked> {
        let start = Instant::now();
        let mut made = self.made();
        let (logits, segmentation_kept) =
            self.segmentation(frames, &cutout.prompts, clip, &mut made)?;
        let (width, height) = frames.size();
        let n = frames.len();
        let mut matter = match cutout.edges {
            Edges::Refine => {
                let matter = EdgeMatter::open(self.backend, self.store, self.lock, self.threads)?;
                made.ran(matter.run_record().cloned().expect("opened from the store"));
                Some(matter)
            }
            Edges::Hard | Edges::Soft => None,
        };
        let mut smoother = match cutout.smooth {
            Some(_) => {
                let flow = Flow::open(self.backend, self.store, self.lock, self.threads)?;
                made.ran(flow.run_record().cloned().expect("opened from the store"));
                Some(flow)
            }
            None => None,
        };
        let mut writer = MatteWriter::new(self.baked, [dim(width)?, dim(height)?]);
        let (mut refined, mut previous): (usize, Option<Plane>) = (0, None);
        let mut previous_frame = None;
        for (i, plane) in logits.iter().enumerate() {
            let mask = Mask::of(plane, width, height);
            let mut alpha = match (&mut matter, cutout.edges) {
                (Some(matter), _) => {
                    let frame = frames.frame(i)?;
                    let crop = matte::crop(&logits, i, width, height);
                    let edge = matter.alpha(&frame, crop)?;
                    let (alpha, counted) = matte::refine(&edge, &mask, cutout.band, cutout.agree);
                    refined += usize::from(counted);
                    alpha
                }
                (None, Edges::Soft) => mask.probability,
                (None, _) => mask.hard,
            };
            if let (Some(flow), Some((amount, tolerance))) = (&mut smoother, cutout.smooth) {
                let frame = frames.frame(i)?;
                if let (Some(before), Some(smoothed)) = (&previous_frame, &previous) {
                    let backward = flow.between(&frame, before)?;
                    let forward = flow.between(before, &frame)?;
                    let valid = flow::consistent(&backward, &forward);
                    alpha = flow::blend(&alpha, smoothed, &backward, &valid, amount, tolerance);
                }
                previous_frame = Some(frame);
            }
            let baked = Alpha::from_unit(dim(width)?, dim(height)?, &alpha.data);
            writer.push(&baked)?;
            previous = Some(alpha);
            (self.progress)(&format!(
                "matte frame {i} of {n} ({:.2} s a frame so far)",
                start.elapsed().as_secs_f64() / (i + 1) as f64
            ));
        }
        let digest = writer.finish(cutout.request.clone(), *clip, made)?;
        Ok(Baked {
            digest,
            refined,
            frames: n,
            segmentation_kept,
            seconds_per_frame: start.elapsed().as_secs_f64() / n.max(1) as f64,
        })
    }
}

/// A clip dimension as an asset writes it.
fn dim(v: usize) -> Result<u32> {
    u32::try_from(v).map_err(|_| Diagnostic::new(Kind::Invalid, format!("a clip {v} px across")))
}

#[cfg(test)]
#[path = "../tests/unit/bake.rs"]
mod tests;
