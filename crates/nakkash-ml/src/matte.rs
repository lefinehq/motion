//! Mattes from a segmentation ([D87]): its mask as it is, its probability,
//! or its edge refined by the edge matting model where the two agree.
//!
//! The refinement, per frame:
//!
//! 1. **Crop.** The object's box over two frames either side, grown by a
//!    fifth of its larger side and 16 px, is cut from the frame and
//!    resampled to 1024², the edge model's input; its alpha is resampled
//!    back into place, nothing outside it.
//! 2. **Gate.** The segmentation's probability, grown by about 16 px and
//!    feathered, multiplies the edge model's alpha, so the model's choice
//!    of what stands out never reaches past the object.
//! 3. **Agree.** Only when the gated alpha's mask and the segmentation's
//!    have an IoU of at least `agree` does the refinement count; otherwise
//!    the frame keeps the segmentation's probability.
//! 4. **Band.** Where it counts, only the band `band` px either side of the
//!    segmentation's edge takes the gated alpha; inside the band's inner
//!    side the object is opaque, outside its outer side clear.
//!
//! Growing, shrinking and feathering run at a quarter of the frame's size,
//! as they were measured.
//!
//! [D87]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d87-an-objects-matte-is-its-segmentation-refined-at-its-edge

use nakkash_baked::GraphRun;
use nakkash_format::Result;

use crate::image::{Filter, Plane, Rgb8, sigmoid};
use crate::{Backend, Graph, ModelLock, ModelStore, Tensor};

/// The edge model's key in the lock.
pub const MODEL: &str = "matte.edges";

/// The side of the square the edge model reads.
pub const INPUT: usize = 1024;

/// The mean and deviation the edge model was trained with, per channel.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DEVIATION: [f32; 3] = [0.229, 0.224, 0.225];

/// Frames either side whose boxes the crop covers.
const CROP_REACH: usize = 2;

/// What a cutout's matte is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edges {
    /// The segmentation refined at its edge.
    Refine,
    /// The segmentation's mask.
    Hard,
    /// The segmentation's probability.
    Soft,
}

/// The edge matting model, open.
pub struct EdgeMatter {
    graph: Box<dyn Graph>,
    run: Option<GraphRun>,
}

impl std::fmt::Debug for EdgeMatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EdgeMatter")
            .field("run", &self.run)
            .finish_non_exhaustive()
    }
}

impl EdgeMatter {
    /// Opens the edge model from `store` through `backend`.
    ///
    /// # Errors
    /// The store's diagnostic for a missing or changed file, and the
    /// backend's for a graph that does not open.
    pub fn open(
        backend: &dyn Backend,
        store: &ModelStore,
        lock: &ModelLock,
        threads: usize,
    ) -> Result<Self> {
        let graph = store.graph(lock, MODEL, "matte", threads)?;
        Ok(Self {
            run: Some(graph.run()),
            graph: backend.open(&graph)?,
        })
    }

    /// An edge matter on a graph already open.
    pub fn from_graph(graph: Box<dyn Graph>) -> Self {
        Self { graph, run: None }
    }

    /// The graph it runs, for a bake's record.
    pub fn run_record(&self) -> Option<&GraphRun> {
        self.run.as_ref()
    }

    /// The edge model's alpha of `frame` within `crop` (`[x, y, width,
    /// height]`), at the frame's size, clear outside the crop.
    ///
    /// # Errors
    /// The graph's diagnostic.
    pub fn alpha(&mut self, frame: &Rgb8, crop: [usize; 4]) -> Result<Plane> {
        let [x, y, w, h] = crop;
        let input = frame.crop(crop).resized(INPUT, INPUT, Filter::Triangle);
        let plane = INPUT * INPUT;
        let mut nchw = vec![0f32; 3 * plane];
        for (i, pixel) in input.data.chunks_exact(3).enumerate() {
            for c in 0..3 {
                nchw[c * plane + i] = (f32::from(pixel[c]) / 255.0 - MEAN[c]) / DEVIATION[c];
            }
        }
        let name = self
            .graph
            .inputs()
            .into_iter()
            .next()
            .unwrap_or_else(|| "input_image".into());
        let mut out = self
            .graph
            .run(vec![(name, Tensor::f32([1, 3, INPUT, INPUT], nchw))])?;
        let (_, logits) = out.0.pop().ok_or_else(|| {
            nakkash_format::Diagnostic::new(
                nakkash_format::Kind::Missing,
                "the edge model gave no output",
            )
        })?;
        let alpha = Plane {
            width: INPUT,
            height: INPUT,
            data: logits.into_f32()?.into_iter().map(sigmoid).collect(),
        }
        .resized(w, h, Filter::Triangle);
        let mut full = Plane::filled(frame.width, frame.height, 0.0);
        for row in 0..h {
            let at = (y + row) * frame.width + x;
            full.data[at..at + w].copy_from_slice(&alpha.data[row * w..(row + 1) * w]);
        }
        Ok(full)
    }
}

/// The segmentation of one frame at the clip's size: its probability and
/// its mask.
#[derive(Debug, Clone, PartialEq)]
pub struct Mask {
    /// The probability, from 0 to 1.
    pub probability: Plane,
    /// The mask: 1 where the logit is above 0, else 0.
    pub hard: Plane,
}

impl Mask {
    /// The mask of 256² `logits`, upsampled bilinearly to `width` by
    /// `height`, as the reference does.
    pub fn of(logits: &Plane, width: usize, height: usize) -> Self {
        let up = logits.resized(width, height, Filter::Triangle);
        Self {
            hard: Plane {
                width,
                height,
                data: up
                    .data
                    .iter()
                    .map(|&v| f32::from(u8::from(v > 0.0)))
                    .collect(),
            },
            probability: Plane {
                width,
                height,
                data: up.data.iter().map(|&v| sigmoid(v)).collect(),
            },
        }
    }
}

/// The crop around the object on frame `index` of the segmentation
/// `logits`, for a clip of `width` by `height`: the union of its boxes
/// over [`CROP_REACH`] frames either side, grown by a fifth of its larger
/// side and 16 px, in the clip; the whole frame when no frame near has the
/// object.
pub fn crop(logits: &[Plane], index: usize, width: usize, height: usize) -> [usize; 4] {
    let mut union: Option<[f32; 4]> = None;
    let from = index.saturating_sub(CROP_REACH);
    let to = (index + CROP_REACH + 1).min(logits.len());
    for plane in &logits[from..to] {
        let (sx, sy) = (
            width as f32 / plane.width as f32,
            height as f32 / plane.height as f32,
        );
        for (i, &v) in plane.data.iter().enumerate() {
            if v > 0.0 {
                let (x, y) = ((i % plane.width) as f32, (i / plane.width) as f32);
                let b = [x * sx, y * sy, (x + 1.0) * sx, (y + 1.0) * sy];
                union = Some(match union {
                    None => b,
                    Some(u) => [
                        u[0].min(b[0]),
                        u[1].min(b[1]),
                        u[2].max(b[2]),
                        u[3].max(b[3]),
                    ],
                });
            }
        }
    }
    let Some([x0, y0, x1, y1]) = union else {
        return [0, 0, width, height];
    };
    let margin = 0.2 * (x1 - x0).max(y1 - y0) + 16.0;
    let left = (x0 - margin).max(0.0).floor() as usize;
    let top = (y0 - margin).max(0.0).floor() as usize;
    let right = ((x1 + margin).min(width as f32).ceil() as usize)
        .max(left + 8)
        .min(width);
    let bottom = ((y1 + margin).min(height as f32).ceil() as usize)
        .max(top + 8)
        .min(height);
    [left, top, right - left, bottom - top]
}

/// `plane` worked on at a quarter of its size by `f`, and brought back.
fn at_quarter(plane: &Plane, f: impl FnOnce(Plane) -> Plane) -> Plane {
    let (w, h) = (plane.width, plane.height);
    let small = plane.resized((w / 4).max(1), (h / 4).max(1), Filter::Triangle);
    f(small).resized(w, h, Filter::Triangle)
}

/// The gate: the probability grown by about 16 px (a 9 px square at a
/// quarter size) and feathered (a Gaussian of 2 px there).
pub fn gate(probability: &Plane) -> Plane {
    at_quarter(probability, |p| p.max_filter(9).gaussian_blur(2.0))
}

/// The band `band` px either side of `hard`'s edge, pixel by pixel.
pub fn edge_band(hard: &Plane, band: f32) -> Vec<bool> {
    let size = 2 * (band / 4.0).round().max(1.0) as usize + 1;
    let grown = at_quarter(hard, |p| p.max_filter(size));
    let shrunk = at_quarter(hard, |p| p.min_filter(size));
    grown
        .data
        .iter()
        .zip(&shrunk.data)
        .map(|(&g, &s)| g > 0.0 && s < 254.0 / 255.0)
        .collect()
}

/// One frame's refined matte from the edge model's `alpha` and the
/// segmentation's `mask`, and whether the refinement counted (the two
/// agreed).
pub fn refine(alpha: &Plane, mask: &Mask, band: f32, agree: f32) -> (Plane, bool) {
    let gate = gate(&mask.probability);
    let gated: Vec<f32> = alpha
        .data
        .iter()
        .zip(&gate.data)
        .map(|(a, g)| a * g)
        .collect();
    let (mut both, mut either) = (0u64, 0u64);
    for (g, h) in gated.iter().zip(&mask.hard.data) {
        let (g, h) = (*g > 0.5, *h > 0.5);
        both += u64::from(g && h);
        either += u64::from(g || h);
    }
    let iou = if either == 0 {
        1.0
    } else {
        both as f64 / either as f64
    };
    if iou < f64::from(agree) {
        return (mask.probability.clone(), false);
    }
    let band = edge_band(&mask.hard, band);
    let data = gated
        .iter()
        .zip(&band)
        .zip(&mask.hard.data)
        .map(|((&g, &b), &h)| if b { g } else { h })
        .collect();
    (
        Plane {
            width: alpha.width,
            height: alpha.height,
            data,
        },
        true,
    )
}

#[cfg(test)]
#[path = "../tests/unit/matte.rs"]
mod tests;
