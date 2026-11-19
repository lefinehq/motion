//! [`Segmenter`]: finds an object from prompts and follows its mask through
//! a clip ([D87]), with the segmenter's memory kept here, as the reference
//! implementation keeps it.
//!
//! The model is five graphs (`segment.video` in the lock): an image encoder,
//! two mask decoders (one mask, or three with the most stable chosen), a
//! memory encoder and a memory attention. Per frame:
//!
//! 1. The image encoder reads the frame at 1024², resampled bicubically.
//! 2. On a prompted frame the decoder reads the prompt and the frame's
//!    features plus a learned "no memory" embedding. On any other frame the
//!    memory attention first conditions the features on the memory: every
//!    prompted frame's spatial memory, then up to six frames before it in
//!    the direction of travel, then up to sixteen object pointers, each with
//!    its distance in time.
//! 3. The memory encoder turns the frame's mask into its spatial memory,
//!    rounded to bfloat16 as the reference stores it.
//!
//! Every prompted frame is an initial conditioning frame: computed from its
//! prompt alone, then remembered by the whole clip. Propagation runs from
//! the first prompted frame forward to the end, then from it backward to
//! the start; the backward pass reads the forward pass's memories of the
//! frames after the first prompt, as the reference's shared memory does.
//! So the result is a function of the prompts, not of the order they were
//! given in.
//!
//! [D87]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d87-an-objects-matte-is-its-segmentation-refined-at-its-edge

use std::collections::BTreeMap;

use nakkash_baked::GraphRun;
use nakkash_format::{Diagnostic, Kind, Result};

use crate::image::{Filter, Plane, Rgb8};
use crate::{Backend, Graph, ModelLock, ModelStore, Tensor};

/// The model's key in the lock.
pub const MODEL: &str = "segment.video";

/// The side of the square the image encoder reads.
pub const INPUT: usize = 1024;

/// The side of the square of mask logits the decoders give.
pub const MASK: usize = 256;

/// Spatial memories per frame: the prompted frames' and six before.
const MASK_MEMORIES: usize = 7;

/// Object pointers at most.
const POINTERS: usize = 16;

/// The mean and deviation the image encoder was trained with, per channel.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DEVIATION: [f32; 3] = [0.229, 0.224, 0.225];

/// The frames of a clip, read in any order.
pub trait Frames {
    /// How many frames it has.
    fn len(&self) -> usize;

    /// Whether it has none.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Its frame size in pixels.
    fn size(&self) -> (usize, usize);

    /// Frame `index`, in the order frames show.
    ///
    /// # Errors
    /// The clip's diagnostic when the frame does not decode.
    fn frame(&mut self, index: usize) -> Result<Rgb8>;
}

/// What a prompt points at on one frame, in clip pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    /// The clip frame, in the order frames show.
    pub frame: usize,
    /// A box around the object: left, top, right, bottom.
    pub area: Option<[f32; 4]>,
    /// Points on the object.
    pub include: Vec<[f32; 2]>,
    /// Points not on it.
    pub exclude: Vec<[f32; 2]>,
}

impl Prompt {
    /// Its points in the decoder's 1024² square, and their labels: a box as
    /// its corners (2, 3), then points on the object (1) and off it (0).
    fn points(&self, (width, height): (usize, usize)) -> (Vec<f32>, Vec<i32>) {
        let (sx, sy) = (INPUT as f32 / width as f32, INPUT as f32 / height as f32);
        let (mut coords, mut labels) = (Vec::new(), Vec::new());
        if let Some([x0, y0, x1, y1]) = self.area {
            coords.extend([x0 * sx, y0 * sy, x1 * sx, y1 * sy]);
            labels.extend([2, 3]);
        }
        for (points, label) in [(&self.include, 1), (&self.exclude, 0)] {
            for [x, y] in points {
                coords.extend([x * sx, y * sy]);
                labels.push(label);
            }
        }
        (coords, labels)
    }
}

/// One frame's memory: its spatial memory and its object pointer.
struct Memory {
    features: Vec<f32>,
    pointer: Vec<f32>,
}

/// One frame's encoding.
struct Encoded {
    s0: Vec<f32>,
    s1: Vec<f32>,
    vision: Vec<f32>,
    position: Vec<f32>,
}

/// The segmenter's graphs, open.
pub struct Segmenter {
    encoder: Box<dyn Graph>,
    decoder_multi: Box<dyn Graph>,
    decoder_single: Box<dyn Graph>,
    memory_encoder: Box<dyn Graph>,
    memory_attention: Box<dyn Graph>,
    no_memory: Vec<f32>,
    runs: Vec<GraphRun>,
}

impl std::fmt::Debug for Segmenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segmenter")
            .field("runs", &self.runs)
            .finish_non_exhaustive()
    }
}

/// Runs `graph` on the inputs it declares among `inputs`.
fn run(graph: &mut dyn Graph, inputs: Vec<(&str, Tensor)>) -> Result<crate::Outputs> {
    let declared = graph.inputs();
    let inputs = inputs
        .into_iter()
        .filter(|(name, _)| declared.iter().any(|d| d == name))
        .map(|(name, tensor)| (name.to_owned(), tensor))
        .collect();
    graph.run(inputs)
}

impl Segmenter {
    /// Opens the segmenter's graphs from `store` through `backend`, on
    /// `threads` threads each.
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
        let mut runs = Vec::new();
        let mut open = |name: &str| -> Result<Box<dyn Graph>> {
            let graph = store.graph(lock, MODEL, name, threads)?;
            runs.push(graph.run());
            backend.open(&graph)
        };
        Ok(Self {
            encoder: open("image-encoder")?,
            decoder_multi: open("decoder-multi")?,
            decoder_single: open("decoder-single")?,
            memory_encoder: open("memory-encoder")?,
            memory_attention: open("memory-attention")?,
            no_memory: store
                .constant(lock, MODEL, "no-memory-embedding")?
                .into_f32()?,
            runs,
        })
    }

    /// A segmenter on graphs already open, for tests and other backends.
    pub fn from_graphs(graphs: [Box<dyn Graph>; 5], no_memory: Vec<f32>) -> Self {
        let [
            encoder,
            decoder_multi,
            decoder_single,
            memory_encoder,
            memory_attention,
        ] = graphs;
        Self {
            encoder,
            decoder_multi,
            decoder_single,
            memory_encoder,
            memory_attention,
            no_memory,
            runs: Vec::new(),
        }
    }

    /// The graphs it runs, for a bake's record.
    pub fn runs(&self) -> &[GraphRun] {
        &self.runs
    }

    /// The object's mask logits on every frame of `frames`, each a 256²
    /// plane, from `prompts`, telling `progress` each frame it finishes.
    ///
    /// # Errors
    /// A [`Kind::Invalid`] diagnostic when there is no prompt, a prompt
    /// points at nothing or past the clip, or two share a frame; the
    /// graphs' and the clip's diagnostics.
    pub fn segment(
        &mut self,
        frames: &mut dyn Frames,
        prompts: &[Prompt],
        mut progress: impl FnMut(usize),
    ) -> Result<Vec<Plane>> {
        let n = frames.len();
        let size = frames.size();
        let mut prompts: Vec<&Prompt> = prompts.iter().collect();
        prompts.sort_by_key(|p| p.frame);
        let first = check_prompts(&prompts, n)?;
        let mut masks: Vec<Option<Plane>> = vec![None; n];
        let mut conditioned: BTreeMap<usize, Memory> = BTreeMap::new();
        let mut position = Vec::new();
        for prompt in &prompts {
            let encoded = self.encode(frames.frame(prompt.frame)?)?;
            let mut features = encoded.vision.clone();
            let plane = 64 * 64;
            for (c, embed) in self.no_memory.iter().enumerate() {
                features[c * plane..(c + 1) * plane]
                    .iter_mut()
                    .for_each(|v| *v += embed);
            }
            let (coords, labels) = prompt.points(size);
            let multimask = labels.len() <= 1;
            let (mask, memory) = self.decode(
                &encoded,
                features,
                coords,
                labels,
                multimask,
                true,
                &mut position,
            )?;
            masks[prompt.frame] = Some(mask);
            conditioned.insert(prompt.frame, memory);
            progress(prompt.frame);
        }
        let mut others: BTreeMap<usize, Memory> = BTreeMap::new();
        let window = POINTERS.max(MASK_MEMORIES);
        for (direction, order) in [
            (1i64, (first + 1..n).collect::<Vec<_>>()),
            (-1, (0..first).rev().collect()),
        ] {
            for f in order {
                if conditioned.contains_key(&f) {
                    continue;
                }
                let encoded = self.encode(frames.frame(f)?)?;
                let features =
                    self.attend(&encoded, f, n, direction, &conditioned, &others, &position)?;
                let (mask, memory) = self.decode(
                    &encoded,
                    features,
                    vec![0.0, 0.0],
                    vec![-1],
                    true,
                    false,
                    &mut position,
                )?;
                masks[f] = Some(mask);
                others.insert(f, memory);
                // Keep what a later frame reads: the window behind this
                // one, and, for the backward pass, the frames just after
                // the first prompt.
                others.retain(|&t, _| {
                    let behind = (f as i64 - t as i64) * direction;
                    (0..=window as i64).contains(&behind) || (t > first && t <= first + window)
                });
                progress(f);
            }
        }
        Ok(masks
            .into_iter()
            .map(|m| m.expect("every frame is prompted or propagated"))
            .collect())
    }

    /// Encodes one frame.
    fn encode(&mut self, frame: Rgb8) -> Result<Encoded> {
        let resized = frame.resized(INPUT, INPUT, Filter::Cubic);
        let plane = INPUT * INPUT;
        let mut nchw = vec![0f32; 3 * plane];
        for (i, pixel) in resized.data.chunks_exact(3).enumerate() {
            for c in 0..3 {
                nchw[c * plane + i] = (f32::from(pixel[c]) / 255.0 - MEAN[c]) / DEVIATION[c];
            }
        }
        let mut out = run(
            self.encoder.as_mut(),
            vec![("image", Tensor::f32([1, 3, INPUT, INPUT], nchw))],
        )?;
        Ok(Encoded {
            s0: out.floats("feat_s0")?,
            s1: out.floats("feat_s1")?,
            vision: out.floats("vision_feat")?,
            position: out.floats("vision_pos")?,
        })
    }

    /// The memory attention's features for frame `f`, travelling in
    /// `direction` through a clip of `n` frames.
    #[allow(clippy::too_many_arguments)]
    fn attend(
        &mut self,
        encoded: &Encoded,
        f: usize,
        n: usize,
        direction: i64,
        conditioned: &BTreeMap<usize, Memory>,
        others: &BTreeMap<usize, Memory>,
        position: &[f32],
    ) -> Result<Vec<f32>> {
        let selection = select(
            f,
            n,
            direction,
            conditioned.keys().copied().collect(),
            others,
        );
        let mut features = Vec::new();
        for (frame, _) in &selection.spatial {
            let memory = conditioned
                .get(frame)
                .or_else(|| others.get(frame))
                .expect("selected");
            features.extend_from_slice(&memory.features);
        }
        let mut pointers = Vec::new();
        for (frame, _) in &selection.pointers {
            let memory = conditioned
                .get(frame)
                .or_else(|| others.get(frame))
                .expect("selected");
            pointers.extend_from_slice(&memory.pointer);
        }
        let (nf, np) = (selection.spatial.len(), selection.pointers.len());
        let mut out = run(
            self.memory_attention.as_mut(),
            vec![
                (
                    "curr",
                    Tensor::f32([1, 256, 64, 64], encoded.vision.clone()),
                ),
                (
                    "curr_pos",
                    Tensor::f32([1, 256, 64, 64], encoded.position.clone()),
                ),
                ("mem_feat", Tensor::f32([nf, 64, 64, 64], features)),
                (
                    "mem_tpos_idx",
                    Tensor::i64([nf], selection.spatial.iter().map(|s| s.1).collect()),
                ),
                (
                    "maskmem_pos",
                    Tensor::f32([1, 64, 64, 64], position.to_vec()),
                ),
                ("obj_ptrs", Tensor::f32([np, 256], pointers)),
                (
                    "obj_ptr_tpos",
                    Tensor::f32([np], selection.pointers.iter().map(|p| p.1).collect()),
                ),
            ],
        )?;
        out.floats("pix_feat")
    }

    /// Decodes a mask from `features` and the points, and encodes its
    /// memory; the first memory encoding also gives the memories' positions.
    #[allow(clippy::too_many_arguments)]
    fn decode(
        &mut self,
        encoded: &Encoded,
        features: Vec<f32>,
        coords: Vec<f32>,
        labels: Vec<i32>,
        multimask: bool,
        prompted: bool,
        position: &mut Vec<f32>,
    ) -> Result<(Plane, Memory)> {
        let k = labels.len();
        let decoder = match multimask {
            true => self.decoder_multi.as_mut(),
            false => self.decoder_single.as_mut(),
        };
        let mut out = run(
            decoder,
            vec![
                ("pix_feat", Tensor::f32([1, 256, 64, 64], features)),
                (
                    "feat_s0",
                    Tensor::f32([1, 32, 256, 256], encoded.s0.clone()),
                ),
                (
                    "feat_s1",
                    Tensor::f32([1, 64, 128, 128], encoded.s1.clone()),
                ),
                ("point_coords", Tensor::f32([1, k, 2], coords)),
                ("point_labels", Tensor::i32([1, k], labels)),
            ],
        )?;
        let low = out.floats("low_res_mask")?;
        let high = out.floats("high_res_mask")?;
        let pointer = out.floats("obj_ptr")?;
        let score = out.floats("obj_score")?;
        let mut memory = run(
            self.memory_encoder.as_mut(),
            vec![
                (
                    "vision_feat",
                    Tensor::f32([1, 256, 64, 64], encoded.vision.clone()),
                ),
                ("high_res_mask", Tensor::f32([1, 1, INPUT, INPUT], high)),
                ("obj_score", Tensor::f32([1, 1], score)),
                (
                    "binarize",
                    Tensor::f32([1], vec![if prompted { 1.0 } else { 0.0 }]),
                ),
            ],
        )?;
        let mut features = memory.floats("maskmem_feat")?;
        if position.is_empty() {
            *position = memory.floats("maskmem_pos")?;
        }
        bfloat16(&mut features);
        if low.len() != MASK * MASK {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "the mask decoder gave {} logits, not {}",
                    low.len(),
                    MASK * MASK
                ),
            ));
        }
        Ok((
            Plane {
                width: MASK,
                height: MASK,
                data: low,
            },
            Memory { features, pointer },
        ))
    }
}

/// The prompted frames, checked, and the first.
fn check_prompts(prompts: &[&Prompt], n: usize) -> Result<usize> {
    let invalid = |why: String| Diagnostic::new(Kind::Invalid, why);
    let first = prompts
        .first()
        .ok_or_else(|| invalid("a cutout or track with no prompt".into()))?
        .frame;
    for pair in prompts.windows(2) {
        if pair[0].frame == pair[1].frame {
            return Err(invalid(format!("two prompts on frame {}", pair[0].frame)));
        }
    }
    for prompt in prompts {
        if prompt.frame >= n {
            return Err(invalid(format!(
                "a prompt on frame {} of a clip of {n} frames",
                prompt.frame
            )));
        }
        if prompt.area.is_none() && prompt.include.is_empty() {
            return Err(invalid(format!(
                "the prompt on frame {} has no box and no point on the object",
                prompt.frame
            )));
        }
    }
    Ok(first)
}

/// What the memory attention reads for one frame: spatial memories by frame
/// with their temporal index, and pointers by frame with their offset.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Selection {
    pub(crate) spatial: Vec<(usize, i64)>,
    pub(crate) pointers: Vec<(usize, f32)>,
}

/// The memories frame `f` reads, travelling in `direction` (1 forward, −1
/// backward) through `n` frames: as the reference's
/// `_prepare_memory_conditioned_features` chooses them, with every prompted
/// frame selected.
pub(crate) fn select<T>(
    f: usize,
    n: usize,
    direction: i64,
    conditioned: Vec<usize>,
    others: &BTreeMap<usize, T>,
) -> Selection {
    let f = f as i64;
    let mut spatial: Vec<(usize, i64)> = conditioned
        .iter()
        .map(|&c| (c, MASK_MEMORIES as i64 - 1))
        .collect();
    for t_pos in 1..MASK_MEMORIES as i64 {
        let previous = f - direction * (MASK_MEMORIES as i64 - t_pos);
        if previous >= 0 && others.contains_key(&(previous as usize)) {
            spatial.push((previous as usize, MASK_MEMORIES as i64 - t_pos - 1));
        }
    }
    let most = n.min(POINTERS);
    let scale = (most.saturating_sub(1)).max(1) as f32;
    // Only prompted frames already passed point the way.
    let mut pointers: Vec<(usize, f32)> = conditioned
        .iter()
        .filter(|&&c| (f - c as i64) * direction >= 0)
        .map(|&c| (c, ((f - c as i64) * direction) as f32 / scale))
        .collect();
    for t_diff in 1..most as i64 {
        let t = f - direction * t_diff;
        if t < 0 || t >= n as i64 {
            break;
        }
        if others.contains_key(&(t as usize)) {
            pointers.push((t as usize, t_diff as f32 / scale));
        }
    }
    Selection { spatial, pointers }
}

/// Rounds each value to bfloat16, to nearest with ties to even, as the
/// reference stores memories.
pub(crate) fn bfloat16(values: &mut [f32]) {
    for v in values {
        let bits = v.to_bits();
        let rounded = bits.wrapping_add(0x7fff + ((bits >> 16) & 1)) & 0xffff_0000;
        *v = f32::from_bits(rounded);
    }
}

#[cfg(test)]
#[path = "../tests/unit/segment.rs"]
pub(crate) mod tests;
