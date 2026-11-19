//! [`PointTracker`]: the point tracking model (`points.video`), run on a
//! clip through a [`Backend`], as [`FollowPoints`] asks.
//!
//! The model has two graphs. The features graph reads one frame resized
//! to 512², its values from −1 to 1, and gives four grids of features; the
//! tracker graph reads the grids of a window of frames, stacked, and the
//! queries, and gives each query's position, occlusion logit and expected
//! distance on every frame of the window. A point is seen where
//! `(1 − σ(occlusion)) · (1 − σ(expected distance)) > 0.5`, the model's own
//! rule.
//!
//! Positions are continuous: a clip pixel's centre, at whole numbers in
//! clip pixels, lies at `(x + 0.5) · 512 / width` on the model's grid,
//! whose pixel edges are at whole numbers. Queries go in as (frame, y, x),
//! positions come out as (x, y).
//!
//! Features are computed once per frame and kept while the windows that
//! read them run: the windows of [`follow_all`](super::follow_all) move
//! outwards and share [`OVERLAP`](super::OVERLAP) frames, so a frame's
//! features are computed at most twice.

use std::collections::BTreeMap;
use std::ops::Range;

use nakkash_baked::{GraphRun, PointTrack};
use nakkash_format::{Diagnostic, Kind, Result};

use super::follow::{FollowPoints, Query};
use crate::image::{Filter, sigmoid};
use crate::segment::Frames;
use crate::{Backend, Data, Graph, ModelLock, ModelStore, Tensor};

/// The point tracking model's key in the lock.
pub const MODEL: &str = "points.video";

/// The side of the square the model reads frames at.
pub const SIZE: usize = 512;

/// The most frames one tracker run spans: what was measured, 48 frames of
/// 34 points in 3.6 GB.
pub const WINDOW: usize = 48;

/// The features graph's outputs, in the order the tracker reads them.
const GRIDS: [&str; 4] = ["low0", "hi0", "low1", "hi1"];

/// The point tracking model, open on a clip.
pub struct PointTracker<'a> {
    features: Box<dyn Graph>,
    tracker: Box<dyn Graph>,
    frames: Box<dyn Frames + 'a>,
    size: (usize, usize),
    kept: BTreeMap<usize, Vec<Tensor>>,
    runs: Vec<GraphRun>,
}

impl std::fmt::Debug for PointTracker<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PointTracker")
            .field("size", &self.size)
            .field("runs", &self.runs)
            .finish_non_exhaustive()
    }
}

impl<'a> PointTracker<'a> {
    /// Opens the model from `store` through `backend` on the clip
    /// `frames`.
    ///
    /// # Errors
    /// The store's diagnostic for a missing or changed file, and the
    /// backend's for a graph that does not open.
    pub fn open(
        backend: &dyn Backend,
        store: &ModelStore,
        lock: &ModelLock,
        threads: usize,
        frames: Box<dyn Frames + 'a>,
    ) -> Result<Self> {
        let features = store.graph(lock, MODEL, "features", threads)?;
        let tracker = store.graph(lock, MODEL, "tracker", threads)?;
        let runs = vec![features.run(), tracker.run()];
        Ok(Self {
            features: backend.open(&features)?,
            tracker: backend.open(&tracker)?,
            size: frames.size(),
            frames,
            kept: BTreeMap::new(),
            runs,
        })
    }

    /// A tracker on graphs already open: the features graph, then the
    /// tracker graph.
    pub fn from_graphs(
        features: Box<dyn Graph>,
        tracker: Box<dyn Graph>,
        frames: Box<dyn Frames + 'a>,
    ) -> Self {
        Self {
            features,
            tracker,
            size: frames.size(),
            frames,
            kept: BTreeMap::new(),
            runs: Vec::new(),
        }
    }

    /// The graphs it runs, for a bake's record.
    pub fn runs(&self) -> &[GraphRun] {
        &self.runs
    }

    /// The four feature grids of clip frame `f`, computed if not kept.
    fn grids(&mut self, f: usize) -> Result<&[Tensor]> {
        if !self.kept.contains_key(&f) {
            let frame = self.frames.frame(f)?.resized(SIZE, SIZE, Filter::Triangle);
            let values = frame
                .data
                .iter()
                .map(|&v| f32::from(v) / 255.0 * 2.0 - 1.0)
                .collect();
            let name = self
                .features
                .inputs()
                .into_iter()
                .next()
                .unwrap_or_else(|| "frames".into());
            let mut out = self
                .features
                .run(vec![(name, Tensor::f32([1, SIZE, SIZE, 3], values))])?;
            let grids = GRIDS
                .iter()
                .map(|g| out.take(g))
                .collect::<Result<Vec<_>>>()?;
            self.kept.insert(f, grids);
        }
        Ok(&self.kept[&f])
    }
}

impl FollowPoints for PointTracker<'_> {
    fn frames(&self) -> usize {
        self.frames.len()
    }

    fn window(&self) -> usize {
        WINDOW
    }

    fn follow(&mut self, frames: Range<usize>, queries: &[Query]) -> Result<Vec<PointTrack>> {
        let t = frames.len();
        if t == 0 || frames.end > self.frames.len() || t > WINDOW {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "a point tracker run over frames {frames:?} of a clip of {}, at most {WINDOW} at once",
                    self.frames.len()
                ),
            ));
        }
        // Features of frames outside this window are let go: windows only
        // move outwards from where they started.
        self.kept.retain(|f, _| frames.contains(f));
        let declared = self.tracker.inputs();
        let mut stacked: Vec<(String, Tensor)> = Vec::with_capacity(5);
        for (g, name) in GRIDS.iter().enumerate() {
            if !declared.iter().any(|d| d == name) {
                continue;
            }
            let mut values = Vec::new();
            let mut shape = Vec::new();
            for f in frames.clone() {
                let grid = &self.grids(f)?[g];
                shape.clone_from(&grid.shape);
                match &grid.data {
                    Data::F32(v) => values.extend_from_slice(v),
                    _ => {
                        return Err(Diagnostic::new(
                            Kind::Invalid,
                            format!("the point features' {name} is not floats"),
                        ));
                    }
                }
            }
            // [1, h, w, c] per frame, stacked to [t, h, w, c].
            shape[0] = t;
            stacked.push(((*name).into(), Tensor::f32(shape, values)));
        }
        let (w, h) = self.size;
        let (sx, sy) = (SIZE as f64 / w as f64, SIZE as f64 / h as f64);
        let mut points = Vec::with_capacity(queries.len() * 3);
        for q in queries {
            let f = q.frame as usize;
            if !frames.contains(&f) {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!("a query on frame {f}, outside the run over {frames:?}"),
                ));
            }
            points.extend([
                (f - frames.start) as f32,
                ((q.at[1] + 0.5) * sy) as f32,
                ((q.at[0] + 0.5) * sx) as f32,
            ]);
        }
        stacked.push((
            "query_points".into(),
            Tensor::f32([queries.len(), 3], points),
        ));
        let mut out = self.tracker.run(stacked)?;
        let tracks = out.floats("tracks")?;
        let occlusion = out.floats("occlusion")?;
        let distance = out.floats("expected_dist")?;
        let n = queries.len();
        if tracks.len() != n * t * 2 || occlusion.len() != n * t || distance.len() != n * t {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!("the point tracker gave tracks of another shape than {n} by {t}"),
            ));
        }
        Ok(queries
            .iter()
            .enumerate()
            .map(|(i, q)| PointTrack {
                frame: q.frame,
                at: (0..t)
                    .map(|k| {
                        let p = &tracks[(i * t + k) * 2..(i * t + k) * 2 + 2];
                        [f64::from(p[0]) / sx - 0.5, f64::from(p[1]) / sy - 0.5]
                    })
                    .collect(),
                seen: (0..t)
                    .map(|k| {
                        let j = i * t + k;
                        (1.0 - sigmoid(occlusion[j])) * (1.0 - sigmoid(distance[j])) > 0.5
                    })
                    .collect(),
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/track/tracker.rs"]
mod tests;
